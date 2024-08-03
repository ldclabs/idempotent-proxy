use candid::{CandidType, Nat, Principal};
use ciborium::into_writer;
use futures::FutureExt;
use ic_cdk::api::management_canister::http_request::{CanisterHttpRequestArgument, HttpResponse};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::{agent::Agent, cose::CoseClient, store};

const MILLISECONDS: u64 = 1_000_000;

#[derive(CandidType, Deserialize, Serialize)]
pub struct StateInfo {
    pub ecdsa_key_name: String,
    pub proxy_token_public_key: String,
    pub proxy_token_refresh_interval: u64, // seconds
    pub agents: Vec<Agent>,
    pub managers: BTreeSet<Principal>,
    pub callers: u64,
    pub subnet_size: u64,
    pub service_fee: u64, // in cycles
    pub incoming_cycles: u128,
    pub uncollectible_cycles: u128,
    pub cose: Option<CoseClient>,
}

#[ic_cdk::query]
fn state_info() -> StateInfo {
    store::state::with(|s| StateInfo {
        ecdsa_key_name: s.ecdsa_key_name.clone(),
        proxy_token_public_key: s.proxy_token_public_key.clone(),
        proxy_token_refresh_interval: s.proxy_token_refresh_interval,
        agents: s
            .agents
            .iter()
            .map(|a| Agent {
                name: a.name.clone(),
                endpoint: a.endpoint.clone(),
                max_cycles: a.max_cycles,
                proxy_token: None,
            })
            .collect(),
        managers: s.managers.clone(),
        callers: s.callers.len() as u64,
        subnet_size: s.subnet_size,
        service_fee: s.service_fee,
        incoming_cycles: s.incoming_cycles,
        uncollectible_cycles: s.uncollectible_cycles,
        cose: s.cose.clone(),
    })
}

#[ic_cdk::query]
fn caller_info(id: Principal) -> Option<(u128, u64)> {
    store::state::with(|s| s.callers.get(&id).copied())
}

#[ic_cdk::query]
async fn proxy_http_request_cost(req: CanisterHttpRequestArgument) -> u128 {
    let calc = store::state::cycles_calculator();
    calc.ingress_cost(ic_cdk::api::call::arg_data_raw_size())
        + calc.http_outcall_request_cost(calc.count_request_bytes(&req), 1)
        + calc.http_outcall_response_cost(req.max_response_bytes.unwrap_or(10240) as usize, 1)
}

#[ic_cdk::query]
async fn parallel_call_cost(req: CanisterHttpRequestArgument) -> u128 {
    let agents = store::state::get_agents();
    let calc = store::state::cycles_calculator();
    calc.ingress_cost(ic_cdk::api::call::arg_data_raw_size())
        + calc.http_outcall_request_cost(calc.count_request_bytes(&req), agents.len())
        + calc.http_outcall_response_cost(
            req.max_response_bytes.unwrap_or(10240) as usize,
            agents.len(),
        )
}

/// Proxy HTTP request by all agents in sequence until one returns an status <= 500 result.
#[ic_cdk::update]
async fn proxy_http_request(req: CanisterHttpRequestArgument) -> HttpResponse {
    let caller = ic_cdk::caller();
    if !store::state::is_allowed(&caller) {
        return HttpResponse {
            status: Nat::from(403u64),
            body: "caller is not allowed".as_bytes().to_vec(),
            headers: vec![],
        };
    }

    let agents = store::state::get_agents();
    if agents.is_empty() {
        return HttpResponse {
            status: Nat::from(503u64),
            body: "no agents available".as_bytes().to_vec(),
            headers: vec![],
        };
    }

    let balance = ic_cdk::api::call::msg_cycles_available128();
    let calc = store::state::cycles_calculator();
    store::state::receive_cycles(
        calc.ingress_cost(ic_cdk::api::call::arg_data_raw_size()),
        false,
    );

    let req_size = calc.count_request_bytes(&req);
    let mut last_err: Option<HttpResponse> = None;
    for agent in agents {
        store::state::receive_cycles(calc.http_outcall_request_cost(req_size, 1), false);
        match agent.call(req.clone()).await {
            Ok(res) => {
                let cycles = calc.http_outcall_response_cost(calc.count_response_bytes(&res), 1);
                store::state::receive_cycles(cycles, true);
                store::state::update_caller_state(
                    &caller,
                    balance - ic_cdk::api::call::msg_cycles_available128(),
                    ic_cdk::api::time() / MILLISECONDS,
                );
                return res;
            }
            Err(res) => last_err = Some(res),
        }
    }

    store::state::update_caller_state(
        &caller,
        balance - ic_cdk::api::call::msg_cycles_available128(),
        ic_cdk::api::time() / MILLISECONDS,
    );
    last_err.unwrap()
}

/// Proxy HTTP request by all agents in parallel and return the result if all are the same,
/// or a 500 HttpResponse with all result.
#[ic_cdk::update]
async fn parallel_call_all_ok(req: CanisterHttpRequestArgument) -> HttpResponse {
    let caller = ic_cdk::caller();
    if !store::state::is_allowed(&caller) {
        return HttpResponse {
            status: Nat::from(403u64),
            body: "caller is not allowed".as_bytes().to_vec(),
            headers: vec![],
        };
    }

    let agents = store::state::get_agents();
    if agents.is_empty() {
        return HttpResponse {
            status: Nat::from(503u64),
            body: "no agents available".as_bytes().to_vec(),
            headers: vec![],
        };
    }

    let balance = ic_cdk::api::call::msg_cycles_available128();
    let calc = store::state::cycles_calculator();
    let cycles = calc.ingress_cost(ic_cdk::api::call::arg_data_raw_size())
        + calc.http_outcall_request_cost(calc.count_request_bytes(&req), agents.len());
    store::state::receive_cycles(cycles, false);

    let results =
        futures::future::try_join_all(agents.iter().map(|agent| agent.call(req.clone()))).await;
    let result = match results {
        Err(res) => res,
        Ok(res) => {
            let mut results = res.into_iter();
            let base_result = results.next().unwrap_or_else(|| HttpResponse {
                status: Nat::from(503u64),
                body: "no agents available".as_bytes().to_vec(),
                headers: vec![],
            });

            let cycles = calc
                .http_outcall_response_cost(calc.count_response_bytes(&base_result), agents.len());
            store::state::receive_cycles(cycles, true);

            let mut inconsistent_results: Vec<_> =
                results.filter(|result| result != &base_result).collect();
            if !inconsistent_results.is_empty() {
                inconsistent_results.push(base_result);
                let mut buf = vec![];
                into_writer(&inconsistent_results, &mut buf)
                    .expect("failed to encode inconsistent results");
                HttpResponse {
                    status: Nat::from(500u64),
                    body: buf,
                    headers: vec![],
                }
            } else {
                base_result
            }
        }
    };

    store::state::update_caller_state(
        &caller,
        balance - ic_cdk::api::call::msg_cycles_available128(),
        ic_cdk::api::time() / MILLISECONDS,
    );
    result
}

/// Proxy HTTP request by all agents in parallel and return the first (status <= 500) result.
#[ic_cdk::update]
async fn parallel_call_any_ok(req: CanisterHttpRequestArgument) -> HttpResponse {
    let caller = ic_cdk::caller();
    if !store::state::is_allowed(&caller) {
        return HttpResponse {
            status: Nat::from(403u64),
            body: "caller is not allowed".as_bytes().to_vec(),
            headers: vec![],
        };
    }

    let agents = store::state::get_agents();
    if agents.is_empty() {
        return HttpResponse {
            status: Nat::from(503u64),
            body: "no agents available".as_bytes().to_vec(),
            headers: vec![],
        };
    }

    let balance = ic_cdk::api::call::msg_cycles_available128();
    let calc = store::state::cycles_calculator();
    let cycles = calc.ingress_cost(ic_cdk::api::call::arg_data_raw_size())
        + calc.http_outcall_request_cost(calc.count_request_bytes(&req), agents.len());
    store::state::receive_cycles(cycles, false);

    let result =
        futures::future::select_ok(agents.iter().map(|agent| agent.call(req.clone()).boxed()))
            .await;
    let result = match result {
        Ok((res, _)) => {
            let cycles =
                calc.http_outcall_response_cost(calc.count_response_bytes(&res), agents.len());
            store::state::receive_cycles(cycles, true);
            res
        }
        Err(res) => res,
    };

    store::state::update_caller_state(
        &caller,
        balance - ic_cdk::api::call::msg_cycles_available128(),
        ic_cdk::api::time() / MILLISECONDS,
    );
    result
}
