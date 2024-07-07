use candid::{Nat, Principal};
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
fn admin_add_canister(canister: Principal) -> Result<bool, String> {
    store::state::with_mut(|r| Ok(r.allowed_canisters.insert(canister)))
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
fn admin_remove_canister(canister: Principal) -> Result<bool, String> {
    store::state::with_mut(|r| Ok(r.allowed_canisters.remove(&canister)))
}

#[ic_cdk::query]
fn get_state() -> Result<store::State, ()> {
    let mut s = store::state::with(|s| s.clone());
    if is_controller_or_manager().is_err() {
        s.agents.clear();
    }
    Ok(s)
}

#[ic_cdk::update]
async fn proxy_http_request(req: CanisterHttpRequestArgument) -> HttpResponse {
    if !store::state::is_allowed(&ic_cdk::caller()) {
        return HttpResponse {
            status: Nat::from(403u64),
            body: "caller is not allowed".as_bytes().to_vec(),
            headers: vec![],
        };
    }

    let agent = store::state::get_agent();
    agent.call(req).await
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
