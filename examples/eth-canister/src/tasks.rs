use std::collections::BTreeMap;

use crate::{agent, ecdsa, store, SECONDS};

pub const REFRESH_PROXY_TOKEN_INTERVAL: u64 = 60 * 60; // 60 minutes

pub async fn refresh_proxy_token() {
    let (ecdsa_key_name, rpc_agent) =
        store::state::with(|s| (s.ecdsa_key_name.clone(), s.rpc_agents.clone()));
    update_proxy_token(ecdsa_key_name, rpc_agent).await;
}

pub async fn update_proxy_token(ecdsa_key_name: String, mut rpc_agents: Vec<agent::RPCAgent>) {
    if rpc_agents.is_empty() {
        return;
    }

    let mut tokens: BTreeMap<String, String> = BTreeMap::new();
    for agent in rpc_agents.iter_mut() {
        if let Some(token) = tokens.get(&agent.name) {
            agent.proxy_token = Some(token.clone());
            continue;
        }

        let token = ecdsa::sign_proxy_token(
            &ecdsa_key_name,
            (ic_cdk::api::time() / SECONDS) + REFRESH_PROXY_TOKEN_INTERVAL + 120,
            &agent.name,
        )
        .await
        .expect("failed to sign proxy token");
        tokens.insert(agent.name.clone(), token.clone());
        agent.proxy_token = Some(token);
    }

    store::state::with_mut(|r| r.rpc_agents = rpc_agents);
}
