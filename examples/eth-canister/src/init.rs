use candid::CandidType;
use serde::Deserialize;
use std::time::Duration;

use crate::{store, tasks};

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ChainArgs {
    Init(InitArgs),
    Upgrade(UpgradeArgs),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    ecdsa_key_name: String, // Use "dfx_test_key" for local replica and "test_key_1" for a testing key for testnet and mainnet
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct UpgradeArgs {}

#[ic_cdk::init]
fn init(args: Option<ChainArgs>) {
    match args.expect("Init args is missing") {
        ChainArgs::Init(args) => {
            store::state::with_mut(|s| {
                s.ecdsa_key_name = args.ecdsa_key_name;
            });
        }
        ChainArgs::Upgrade(_) => {
            ic_cdk::trap(
                "Cannot initialize the canister with an Upgrade args. Please provide an Init args.",
            );
        }
    }

    ic_cdk_timers::set_timer(Duration::from_secs(0), || {
        ic_cdk::spawn(async {
            store::state::init_ecdsa_public_key().await;
            tasks::refresh_proxy_token().await;
        })
    });

    ic_cdk_timers::set_timer_interval(
        Duration::from_secs(tasks::REFRESH_PROXY_TOKEN_INTERVAL),
        || ic_cdk::spawn(tasks::refresh_proxy_token()),
    );
}

#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    store::state::save();
}

#[ic_cdk::post_upgrade]
fn post_upgrade(args: Option<ChainArgs>) {
    store::state::load();

    match args {
        Some(ChainArgs::Upgrade(_args)) => {}
        Some(ChainArgs::Init(_)) => {
            ic_cdk::trap(
                "Cannot upgrade the canister with an Init args. Please provide an Upgrade args.",
            );
        }
        _ => {}
    }

    ic_cdk_timers::set_timer(Duration::from_secs(0), || {
        ic_cdk::spawn(async {
            tasks::refresh_proxy_token().await;
        })
    });

    ic_cdk_timers::set_timer_interval(
        Duration::from_secs(tasks::REFRESH_PROXY_TOKEN_INTERVAL),
        || ic_cdk::spawn(tasks::refresh_proxy_token()),
    );
}
