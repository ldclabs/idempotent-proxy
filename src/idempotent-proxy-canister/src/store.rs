use base64::{engine::general_purpose::URL_SAFE_NO_PAD as base64_url, Engine};
use candid::Principal;
use ciborium::{from_reader, into_writer};
use ic_cose_types::cose::{format_error, sha3_256};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableCell, Storable,
};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

use crate::{
    agent::Agent,
    cose::CoseClient,
    cycles::Calculator,
    ecdsa::{public_key_with, sign_with},
};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct State {
    pub ecdsa_key_name: String,
    pub proxy_token_public_key: String,
    pub proxy_token_refresh_interval: u64, // seconds
    pub agents: Vec<Agent>,
    pub managers: BTreeSet<Principal>,
    pub allowed_callers: BTreeSet<Principal>, //deprecated
    #[serde(default)]
    pub callers: BTreeMap<Principal, (u128, u64)>,
    #[serde(default)]
    pub subnet_size: u64,
    #[serde(default)]
    pub service_fee: u64, // in cycles
    #[serde(default)]
    pub incoming_cycles: u128,
    #[serde(default)]
    pub uncollectible_cycles: u128,

    #[serde(default)]
    pub cose: Option<CoseClient>,
}

impl State {
    pub fn signer(&self) -> Signer {
        Signer {
            key_name: self.ecdsa_key_name.clone(),
            cose: self.cose.clone(),
        }
    }
}

impl Storable for State {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode State data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode State data")
    }
}

pub struct Signer {
    pub key_name: String,
    pub cose: Option<CoseClient>,
}

static SIGN_PROXY_TOKEN_PATH: &[u8] = b"sign_proxy_token";

impl Signer {
    pub async fn ecdsa_public_key(&self) -> Result<String, String> {
        match self.cose {
            Some(ref cose) => cose
                .ecdsa_public_key(vec![ByteBuf::from(SIGN_PROXY_TOKEN_PATH)])
                .await
                .map(|v| base64_url.encode(v)),
            None => public_key_with(&self.key_name, vec![SIGN_PROXY_TOKEN_PATH.to_vec()])
                .await
                .map(|v| base64_url.encode(v.public_key)),
        }
    }

    // use Idempotent Proxy's Token: Token(pub u64, pub String, pub ByteBuf);
    // https://github.com/ldclabs/idempotent-proxy/blob/main/src/idempotent-proxy-types/src/auth.rs#L15
    pub async fn sign_proxy_token(
        &self,
        expire_at: u64, // UNIX timestamp, in seconds
        message: &str,  // use RPCAgent.name as message
    ) -> Result<String, String> {
        let mut buf: Vec<u8> = Vec::new();
        into_writer(&(expire_at, message), &mut buf)
            .expect("failed to encode Token in CBOR format");
        let digest = sha3_256(&buf);

        let sig = match self.cose {
            Some(ref cose) => {
                cose.ecdsa_sign(
                    vec![ByteBuf::from(SIGN_PROXY_TOKEN_PATH)],
                    ByteBuf::from(digest),
                )
                .await
            }
            None => sign_with(&self.key_name, vec![SIGN_PROXY_TOKEN_PATH.to_vec()], digest)
                .await
                .map(ByteBuf::from),
        };

        buf.clear();
        into_writer(&(expire_at, message, sig?), &mut buf).map_err(format_error)?;
        Ok(base64_url.encode(buf))
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());

    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static STATE_STORE: RefCell<StableCell<State, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(STATE_MEMORY_ID)),
            State::default()
        ).expect("failed to init STATE_STORE store")
    );

}

pub mod state {
    use super::*;

    pub fn get_agents() -> Vec<Agent> {
        STATE.with(|r| r.borrow().agents.clone())
    }

    pub fn cycles_calculator() -> Calculator {
        STATE.with(|r| {
            let s = r.borrow();
            Calculator {
                subnet_size: s.subnet_size,
                service_fee: s.service_fee,
            }
        })
    }

    pub fn is_manager(caller: &Principal) -> bool {
        STATE.with(|r| r.borrow().managers.contains(caller))
    }

    pub fn is_allowed(caller: &Principal) -> bool {
        STATE.with(|r| r.borrow().callers.contains_key(caller))
    }

    pub fn update_caller_state(caller: &Principal, cycles: u128, now_ms: u64) {
        STATE.with(|r| {
            r.borrow_mut().callers.get_mut(caller).map(|v| {
                v.0 = v.0.saturating_add(cycles);
                v.1 = now_ms;
            })
        });
    }

    pub fn with<R>(f: impl FnOnce(&State) -> R) -> R {
        STATE.with(|r| f(&r.borrow()))
    }

    pub fn with_mut<R>(f: impl FnOnce(&mut State) -> R) -> R {
        STATE.with(|r| f(&mut r.borrow_mut()))
    }

    pub fn receive_cycles(cycles: u128, ignore_insufficient: bool) {
        if cycles == 0 {
            return;
        }

        let received = ic_cdk::api::call::msg_cycles_accept128(cycles);
        with_mut(|r| {
            r.incoming_cycles = r.incoming_cycles.saturating_add(received);
            if cycles > received {
                r.uncollectible_cycles = r.uncollectible_cycles.saturating_add(cycles - received);

                if !ignore_insufficient {
                    ic_cdk::trap("insufficient cycles");
                }
            }
        });
    }

    pub fn load() {
        STATE_STORE.with(|r| {
            let mut s = r.borrow().get().clone();
            if !s.allowed_callers.is_empty() {
                s.allowed_callers.iter().for_each(|p| {
                    s.callers.entry(*p).or_insert((0, 0));
                });
                s.allowed_callers.clear();
            }

            STATE.with(|h| {
                *h.borrow_mut() = s;
            });
        });
    }

    pub fn save() {
        STATE.with(|h| {
            STATE_STORE.with(|r| {
                r.borrow_mut()
                    .set(h.borrow().clone())
                    .expect("failed to set STATE data");
            });
        });
    }

    pub async fn init_ecdsa_public_key() {
        let signer = with(|r| r.signer());

        match signer.ecdsa_public_key().await {
            Ok(public_key) => {
                ic_cdk::print("successfully retrieved ECDSA public key");
                with_mut(|r| {
                    r.proxy_token_public_key = public_key;
                });
            }
            Err(err) => {
                ic_cdk::print(format!("failed to retrieve ECDSA public key: {err}"));
            }
        }
    }
}
