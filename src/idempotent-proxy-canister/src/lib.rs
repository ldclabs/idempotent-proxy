use candid::{Nat, Principal};
use ciborium::into_writer;
use futures::FutureExt;
use ic_cdk::api::management_canister::http_request::{CanisterHttpRequestArgument, HttpResponse};
use std::collections::BTreeSet;

mod agent;
mod ecdsa;
mod init;
mod store;
mod tasks;

use crate::init::ChainArgs;

static ANONYMOUS: Principal = Principal::anonymous();

#[ic_cdk::update(guard = "is_controller")]
fn admin_set_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    store::state::with_mut(|r| {
        r.managers = args;
    });
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_set_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    if args.is_empty() {
        return Err("managers cannot be empty".to_string());
    }
    if args.contains(&ANONYMOUS) {
        return Err("anonymous user is not allowed".to_string());
    }
    Ok(())
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
async fn admin_set_agent(agents: Vec<agent::Agent>) -> Result<(), String> {
    if agents.is_empty() {
        return Err("agents cannot be empty".to_string());
    }

    let (ecdsa_key_name, proxy_token_refresh_interval) =
        store::state::with(|s| (s.ecdsa_key_name.clone(), s.proxy_token_refresh_interval));
    tasks::update_proxy_token(ecdsa_key_name, proxy_token_refresh_interval, agents).await;
    Ok(())
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
fn admin_add_caller(id: Principal) -> Result<bool, String> {
    store::state::with_mut(|r| Ok(r.allowed_callers.insert(id)))
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
fn admin_remove_caller(id: Principal) -> Result<bool, String> {
    store::state::with_mut(|r| Ok(r.allowed_callers.remove(&id)))
}

#[ic_cdk::query]
fn get_state() -> Result<store::State, ()> {
    let mut s = store::state::with(|s| s.clone());
    if is_controller_or_manager().is_err() {
        s.agents.iter_mut().for_each(|a| {
            a.proxy_token = None;
        })
    }
    Ok(s)
}

/// Proxy HTTP request by all agents in sequence until one returns an status <= 500 result.
#[ic_cdk::update]
async fn proxy_http_request(req: CanisterHttpRequestArgument) -> HttpResponse {
    if !store::state::is_allowed(&ic_cdk::caller()) {
        return HttpResponse {
            status: Nat::from(403u64),
            body: "caller is not allowed".as_bytes().to_vec(),
            headers: vec![],
        };
    }

    let agents = store::state::get_agents();
    let mut last_err: Option<HttpResponse> = None;
    for agent in agents {
        match agent.call(req.clone()).await {
            Ok(res) => return res,
            Err(res) => last_err = Some(res),
        }
    }

    last_err.unwrap_or_else(|| HttpResponse {
        status: Nat::from(503u64),
        body: "no agents available".as_bytes().to_vec(),
        headers: vec![],
    })
}

/// Proxy HTTP request by all agents in parallel and return the result if all are the same,
/// or a 500 HttpResponse with all result.
#[ic_cdk::update]
async fn parallel_call_all_ok(req: CanisterHttpRequestArgument) -> HttpResponse {
    if !store::state::is_allowed(&ic_cdk::caller()) {
        return HttpResponse {
            status: Nat::from(403u64),
            body: "caller is not allowed".as_bytes().to_vec(),
            headers: vec![],
        };
    }

    let agents = store::state::get_agents();
    let results =
        futures::future::try_join_all(agents.iter().map(|agent| agent.call(req.clone()))).await;

    match results {
        Err(res) => res,
        Ok(res) => {
            let mut results = res.into_iter();
            let base_result = results.next().unwrap_or_else(|| HttpResponse {
                status: Nat::from(503u64),
                body: "no agents available".as_bytes().to_vec(),
                headers: vec![],
            });

            let mut inconsistent_results: Vec<_> =
                results.filter(|result| result != &base_result).collect();
            if !inconsistent_results.is_empty() {
                inconsistent_results.push(base_result);
                let mut buf = vec![];
                into_writer(&inconsistent_results, &mut buf)
                    .expect("failed to encode inconsistent results");
                return HttpResponse {
                    status: Nat::from(500u64),
                    body: buf,
                    headers: vec![],
                };
            }

            base_result
        }
    }
}

/// Proxy HTTP request by all agents in parallel and return the first (status <= 500) result.
#[ic_cdk::update]
async fn parallel_call_one_ok(req: CanisterHttpRequestArgument) -> HttpResponse {
    if !store::state::is_allowed(&ic_cdk::caller()) {
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

    let result =
        futures::future::select_ok(agents.iter().map(|agent| agent.call(req.clone()).boxed()))
            .await;
    match result {
        Ok((res, _)) => res,
        Err(res) => res,
    }
}

fn is_controller() -> Result<(), String> {
    let caller = ic_cdk::caller();
    if ic_cdk::api::is_controller(&caller) {
        Ok(())
    } else {
        Err("user is not a controller".to_string())
    }
}

fn is_controller_or_manager() -> Result<(), String> {
    let caller = ic_cdk::caller();
    if ic_cdk::api::is_controller(&caller) || store::state::is_manager(&caller) {
        Ok(())
    } else {
        Err("user is not a controller or manager".to_string())
    }
}

ic_cdk::export_candid!();
