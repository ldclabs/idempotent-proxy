mod agent;
mod ecdsa;
mod init;
mod jsonrpc;
mod store;
mod tasks;

use crate::{init::ChainArgs, jsonrpc::EthereumRPC};

pub const SECONDS: u64 = 1_000_000_000;

#[ic_cdk::update(guard = "is_controller")]
async fn admin_set_agent(agents: Vec<agent::RPCAgent>) -> Result<(), String> {
    if agents.is_empty() {
        return Err("agents cannot be empty".to_string());
    }

    let ecdsa_key_name = store::state::with(|s| s.ecdsa_key_name.clone());
    tasks::update_proxy_token(ecdsa_key_name, agents).await;
    Ok(())
}

fn is_controller() -> Result<(), String> {
    let caller = ic_cdk::caller();
    if ic_cdk::api::is_controller(&caller) {
        Ok(())
    } else {
        Err("user is not a controller".to_string())
    }
}

#[ic_cdk::query]
fn get_state() -> Result<store::State, ()> {
    let mut s = store::state::with(|s| s.clone());
    if is_controller().is_err() {
        s.rpc_agents.clear();
    }
    Ok(s)
}

#[ic_cdk::update]
async fn eth_chain_id() -> Result<String, String> {
    let agent = store::state::get_agent();
    let res = EthereumRPC::eth_chain_id(&agent, "eth_chain_id".to_string()).await?;
    Ok(res)
}

#[ic_cdk::update]
async fn get_best_block() -> Result<String, String> {
    let agent = store::state::get_agent();
    let ts = ic_cdk::api::time() / SECONDS;
    let key = format!("blk-best-{ts}");
    let res = EthereumRPC::get_best_block(&agent, key).await?;
    let res = serde_json::to_string(&res).map_err(|e| e.to_string())?;
    Ok(res)
}

ic_cdk::export_candid!();
