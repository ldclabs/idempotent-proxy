use candid::{utils::ArgumentEncoder, CandidType, Principal};
use ic_cose_types::types::{PublicKeyInput, PublicKeyOutput, SignInput};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct CoseClient {
    pub id: Principal,
    pub namespace: String,
}

impl CoseClient {
    pub async fn ecdsa_public_key(&self, derivation_path: Vec<ByteBuf>) -> Result<ByteBuf, String> {
        let output: Result<PublicKeyOutput, String> = call(
            self.id,
            "ecdsa_public_key",
            (Some(PublicKeyInput {
                ns: self.namespace.clone(),
                derivation_path,
            }),),
            0,
        )
        .await?;
        let output = output?;
        Ok(output.public_key)
    }

    pub async fn ecdsa_sign(
        &self,
        derivation_path: Vec<ByteBuf>,
        message: ByteBuf,
    ) -> Result<ByteBuf, String> {
        let output: Result<ByteBuf, String> = call(
            self.id,
            "ecdsa_sign",
            (SignInput {
                ns: self.namespace.clone(),
                derivation_path,
                message,
            },),
            0,
        )
        .await?;
        output
    }
}

async fn call<In, Out>(id: Principal, method: &str, args: In, cycles: u128) -> Result<Out, String>
where
    In: ArgumentEncoder + Send,
    Out: candid::CandidType + for<'a> candid::Deserialize<'a>,
{
    let (res,): (Out,) = ic_cdk::api::call::call_with_payment128(id, method, args, cycles)
        .await
        .map_err(|(code, msg)| {
            format!(
                "failed to call {} on {:?}, code: {}, message: {}",
                method, &id, code as u32, msg
            )
        })?;
    Ok(res)
}
