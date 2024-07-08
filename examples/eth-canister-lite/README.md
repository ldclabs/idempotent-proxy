# Example: `eth-canister-lite`

## Running the project locally

If you want to test your project locally, you can use the following commands:

```bash
cd examples/eth-canister-lite
# Starts the replica, running in the background
dfx start --background

# deploy the canister
dfx deploy eth-canister-lite

dfx canister call eth-canister-lite eth_chain_id '()'

dfx canister call eth-canister-lite get_best_block '()'
```

`idempotent-proxy-cf-worker` does not enable `proxy-authorization`, so it can be accessed.

## License
Copyright © 2024 [LDC Labs](https://github.com/ldclabs).

`ldclabs/idempotent-proxy` is licensed under the MIT License. See [LICENSE](../../LICENSE-MIT) for the full license text.