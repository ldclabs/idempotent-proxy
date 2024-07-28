use candid::Principal;
use ic_cose_types::validate_principals;
use std::collections::BTreeSet;

use crate::{agent, is_controller, is_controller_or_manager, store, tasks};

#[ic_cdk::update(guard = "is_controller")]
fn admin_add_managers(mut args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    store::state::with_mut(|r| {
        r.managers.append(&mut args);
        Ok(())
    })
}

#[ic_cdk::update(guard = "is_controller")]
fn admin_remove_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    store::state::with_mut(|r| {
        r.managers.retain(|p| !args.contains(p));
        Ok(())
    })
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
fn admin_add_callers(mut args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    store::state::with_mut(|r| {
        r.allowed_callers.append(&mut args);
        Ok(())
    })
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
fn admin_remove_callers(args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    store::state::with_mut(|r| {
        r.allowed_callers.retain(|p| !args.contains(p));
        Ok(())
    })
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
async fn admin_set_agents(agents: Vec<agent::Agent>) -> Result<(), String> {
    validate_admin_set_agents(agents.clone())?;

    let (signer, proxy_token_refresh_interval) =
        store::state::with(|s| (s.signer(), s.proxy_token_refresh_interval));
    tasks::update_proxy_token(signer, proxy_token_refresh_interval, agents).await;
    Ok(())
}

#[ic_cdk::update]
fn validate_admin_add_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    Ok(())
}

#[ic_cdk::update]
fn validate_admin_remove_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    Ok(())
}

#[ic_cdk::update]
fn validate_admin_set_agents(agents: Vec<agent::Agent>) -> Result<(), String> {
    if agents.is_empty() {
        return Err("agents cannot be empty".to_string());
    }

    Ok(())
}
