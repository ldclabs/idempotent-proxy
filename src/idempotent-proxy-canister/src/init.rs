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
    proxy_token_refresh_interval: u64, // seconds
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct UpgradeArgs {
    proxy_token_refresh_interval: Option<u64>, // seconds
}

#[ic_cdk::init]
fn init(args: Option<ChainArgs>) {
    match args.expect("init args is missing") {
        ChainArgs::Init(args) => {
            store::state::with_mut(|s| {
                s.ecdsa_key_name = args.ecdsa_key_name;
                s.proxy_token_refresh_interval = if args.proxy_token_refresh_interval >= 10 {
                    args.proxy_token_refresh_interval
                } else {
                    3600
                };
            });
        }
        ChainArgs::Upgrade(_) => {
            ic_cdk::trap(
                "cannot initialize the canister with an Upgrade args. Please provide an Init args.",
            );
        }
    }

    ic_cdk_timers::set_timer(Duration::from_secs(0), || {
        ic_cdk::spawn(async {
            store::state::init_ecdsa_public_key().await;
            tasks::refresh_proxy_token().await;
        })
    });

    let proxy_token_refresh_interval = store::state::with(|s| s.proxy_token_refresh_interval);
    ic_cdk_timers::set_timer_interval(Duration::from_secs(proxy_token_refresh_interval), || {
        ic_cdk::spawn(tasks::refresh_proxy_token())
    });
}

#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    store::state::save();
}

#[ic_cdk::post_upgrade]
fn post_upgrade(args: Option<ChainArgs>) {
    store::state::load();

    match args {
        Some(ChainArgs::Upgrade(args)) => {
            store::state::with_mut(|s| {
                if let Some(proxy_token_refresh_interval) = args.proxy_token_refresh_interval {
                    if proxy_token_refresh_interval < 10 {
                        ic_cdk::trap("proxy_token_refresh_interval must be at least 10 seconds");
                    }

                    s.proxy_token_refresh_interval = proxy_token_refresh_interval;
                }
            });
        }
        Some(ChainArgs::Init(_)) => {
            ic_cdk::trap(
                "cannot upgrade the canister with an Init args. Please provide an Upgrade args.",
            );
        }
        _ => {}
    }

    ic_cdk_timers::set_timer(Duration::from_secs(0), || {
        ic_cdk::spawn(async {
            tasks::refresh_proxy_token().await;
        })
    });

    let proxy_token_refresh_interval = store::state::with(|s| s.proxy_token_refresh_interval);
    ic_cdk_timers::set_timer_interval(Duration::from_secs(proxy_token_refresh_interval), || {
        ic_cdk::spawn(tasks::refresh_proxy_token())
    });
}
