# Example: `eth-canister`

## Online Demo

eth-canister: https://a4gq6-oaaaa-aaaab-qaa4q-cai.raw.icp0.io/?id=hpudd-yqaaa-aaaap-ahnbq-cai

## Running the project locally

If you want to test your project locally, you can use the following commands:

```bash
cd examples/eth-canister
# Starts the replica, running in the background
dfx start --background

# deploy the canister
dfx deploy eth-canister --argument "(opt variant {Init =
  record {
    ecdsa_key_name = \"dfx_test_key\";
  }
})"

dfx canister call eth-canister get_state '()'

# set RPC agent
# URL_CF_ETH: https://cloudflare-eth.com
# URL_ANKR_ETH: https://rpc.ankr.com/eth
dfx canister call eth-canister admin_set_agent '
  (vec {
    record {
      name = "LDCLabs";
      endpoint = "https://idempotent-proxy-cf-worker.zensh.workers.dev/URL_CF_ETH";
      max_cycles = 100000000000;
      proxy_token = null;
      api_token = null
    }; record {
      name = "LDCLabs";
      endpoint = "https://idempotent-proxy-cf-worker.zensh.workers.dev/URL_ANKR_ETH";
      max_cycles = 100000000000;
      proxy_token = null;
      api_token = null
    }
  })
'

dfx canister call eth-canister eth_chain_id '()'

dfx canister call eth-canister get_best_block '()'
```

`idempotent-proxy-cf-worker` does not enable `proxy-authorization`, so it can be accessed.

## License
Copyright © 2024 [LDC Labs](https://github.com/ldclabs).

`ldclabs/idempotent-proxy` is licensed under the MIT License. See [LICENSE](../../LICENSE-MIT) for the full license text.