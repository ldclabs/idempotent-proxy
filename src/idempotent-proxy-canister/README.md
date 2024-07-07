# `idempotent-proxy-canister`

## Running the project locally

If you want to test your project locally, you can use the following commands:

```bash
# Starts the replica, running in the background
dfx start --background

# deploy the canister
dfx deploy idempotent-proxy-canister --argument "(opt variant {Init =
  record {
    ecdsa_key_name = \"dfx_test_key\";
    proxy_token_refresh_interval = 3600;
  }
})"

dfx canister call idempotent-proxy-canister get_state '()'

dfx canister call idempotent-proxy-canister admin_set_agent '
  (vec {
    record {
      name = "LDCLabs";
      endpoint = "https://idempotent-proxy-cf-worker.zensh.workers.dev";
      max_cycles = 100000000000;
      proxy_token = null;
    }
  })
'

```

`idempotent-proxy-cf-worker` does not enable `proxy-authorization`, so it can be accessed.

## License
Copyright © 2024 [LDC Labs](https://github.com/ldclabs).

`ldclabs/idempotent-proxy` is licensed under the MIT License. See [LICENSE](../../LICENSE-MIT) for the full license text.