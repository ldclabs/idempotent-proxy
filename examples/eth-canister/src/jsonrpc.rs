use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{to_vec, Value};

use crate::ecdsa::err_string;

pub static APP_AGENT: &str = concat!(
    "Mozilla/5.0 eth-canister ",
    env!("CARGO_PKG_NAME"),
    "/",
    env!("CARGO_PKG_VERSION"),
);

#[async_trait]
pub trait JsonRPCAgent {
    async fn post(&self, idempotency_key: String, body: Vec<u8>) -> Result<Vec<u8>, String>;
}

pub struct EthereumRPC {}

#[derive(Debug, Serialize)]
pub struct RPCRequest<'a> {
    jsonrpc: &'a str,
    method: &'a str,
    params: &'a [Value],
    id: u64,
}

#[derive(Debug, Deserialize)]
pub struct RPCResponse<T> {
    result: Option<T>,
    error: Option<Value>,
}

impl EthereumRPC {
    pub async fn eth_chain_id(
        agent: impl JsonRPCAgent,
        idempotency_key: String,
    ) -> Result<String, String> {
        Self::call(agent, idempotency_key, "eth_chainId", &[]).await
    }

    pub async fn get_best_block(
        agent: impl JsonRPCAgent,
        idempotency_key: String,
    ) -> Result<Value, String> {
        Self::call(
            agent,
            idempotency_key,
            "eth_getBlockByNumber",
            &["latest".into(), false.into()],
        )
        .await
    }

    pub async fn send_raw_transaction(
        agent: impl JsonRPCAgent,
        idempotency_key: String,
        raw_tx: String,
    ) -> Result<String, String> {
        Self::call(
            agent,
            idempotency_key,
            "eth_sendTransaction",
            &[raw_tx.into()],
        )
        .await
    }

    // you can add more methods here

    pub async fn call<T: DeserializeOwned>(
        agent: impl JsonRPCAgent,
        idempotency_key: String,
        method: &str,
        params: &[Value],
    ) -> Result<T, String> {
        let input = RPCRequest {
            jsonrpc: "2.0",
            method,
            params,
            id: 1,
        };
        let input = to_vec(&input).map_err(err_string)?;
        let data = agent.post(idempotency_key, input).await?;

        let output: RPCResponse<T> = serde_json::from_slice(&data).map_err(err_string)?;

        if let Some(error) = output.error {
            return Err(serde_json::to_string(&error).map_err(err_string)?);
        }

        match output.result {
            Some(result) => Ok(result),
            None => serde_json::from_value(Value::Null).map_err(err_string),
        }
    }
}
