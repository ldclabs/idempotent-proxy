mod jsonrpc;

use candid::Principal;

use crate::jsonrpc::EthereumRPC;

#[ic_cdk::update]
async fn eth_chain_id() -> Result<String, String> {
    let rpc = EthereumRPC {
        provider: "URL_CF_ETH".to_string(),
        proxy: Principal::from_text("hpudd-yqaaa-aaaap-ahnbq-cai")
            .map_err(|err| err.to_string())?,
        api_token: None,
    };
    let res = rpc.eth_chain_id("eth_chain_id".to_string()).await?;
    Ok(res)
}

#[ic_cdk::update]
async fn get_best_block() -> Result<String, String> {
    let rpc = EthereumRPC {
        provider: "https://rpc.ankr.com/eth".to_string(),
        proxy: Principal::from_text("hpudd-yqaaa-aaaap-ahnbq-cai")
            .map_err(|err| err.to_string())?,
        api_token: None,
    };
    let ts = ic_cdk::api::time() / 1_000_000_000;
    let key = format!("blk-best-{ts}");
    let res = rpc.get_best_block(key).await?;
    let res = serde_json::to_string(&res).map_err(|e| e.to_string())?;
    Ok(res)
}

ic_cdk::export_candid!();
