use candid::{CandidType, Principal};
use ciborium::{from_reader, into_writer};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableCell, Storable,
};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, cell::RefCell, collections::BTreeSet};

use crate::{agent::Agent, cycles::Calculator, ecdsa::get_proxy_token_public_key};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(CandidType, Clone, Default, Deserialize, Serialize)]
pub struct State {
    pub ecdsa_key_name: String,
    pub proxy_token_public_key: String,
    pub proxy_token_refresh_interval: u64, // seconds
    pub agents: Vec<Agent>,
    pub managers: BTreeSet<Principal>,
    pub allowed_callers: BTreeSet<Principal>,
    #[serde(default)]
    pub subnet_size: u64,
    #[serde(default)]
    pub service_fee: u64, // in cycles
    #[serde(default)]
    pub incoming_cycles: u128,
    #[serde(default)]
    pub uncollectible_cycles: u128,
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
        STATE.with(|r| r.borrow().allowed_callers.contains(caller))
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

    pub async fn init_ecdsa_public_key() {
        let ecdsa_key_name = with(|r| {
            if r.proxy_token_public_key.is_empty() && !r.ecdsa_key_name.is_empty() {
                Some(r.ecdsa_key_name.clone())
            } else {
                None
            }
        });

        if let Some(ecdsa_key_name) = ecdsa_key_name {
            let pk = get_proxy_token_public_key(&ecdsa_key_name)
                .await
                .unwrap_or_else(|err| {
                    ic_cdk::trap(&format!("failed to retrieve ECDSA public key: {err}"))
                });
            with_mut(|r| {
                r.proxy_token_public_key = pk;
            });
        }
    }

    pub fn load() {
        STATE_STORE.with(|r| {
            let s = r.borrow_mut().get().clone();
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
}
