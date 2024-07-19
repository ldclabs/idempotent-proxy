# Idempotent Proxy
Reverse proxy server with build-in idempotency support written in Rust.

## Overview

The idempotent-proxy is a reverse proxy service written in Rust with built-in idempotency support.

When multiple requests with the same idempotency-key arrive within a specific timeframe, only the first request is forwarded to the target service. The response is cached in Redis, and subsequent requests poll Redis to retrieve and return the first request's response.

This service can be used to proxy [HTTPS outcalls](https://internetcomputer.org/docs/current/developer-docs/smart-contracts/advanced-features/https-outcalls/https-outcalls-overview) for [ICP canisters](https://internetcomputer.org/docs/current/developer-docs/smart-contracts/overview/introduction), enabling integration with any Web2 http service. It supports hiding secret information, access control, returning only the necessary headers and, for JSON or CBOR data, allows response filtering based on JSON Mask to return only required fields, thus saving cycles consumption in ICP canisters.

![Idempotent Proxy](../../idempotent-proxy.png)

## Features
- [x] Reverse proxy with build-in idempotency support
- [x] JSON response filtering
- [x] Access control
- [x] Response headers filtering
- [x] HTTPS support
- [x] Running as Cloudflare Worker
- [x] Docker image

## Deploy

### Run proxy in development mode

Run proxy:
```bash
# docker run --name redis -d -p 6379:6379 redis:latest # optional redis
cargo run -p idempotent-proxy-server
```

### Building and running AWS Nitro Enclave image

#### Setup host machine

https://docs.marlin.org/learn/oyster/core-concepts/networking/outgoing

```bash
wget -O vsock-to-ip-transparent http://public.artifacts.marlin.pro/projects/enclaves/vsock-to-ip-transparent_v1.0.0_linux_amd64
chmod +x vsock-to-ip-transparent
./vsock-to-ip-transparent --vsock-addr 3:1200
```

https://docs.marlin.org/learn/oyster/core-concepts/networking/incoming

iptables rules:
```bash
# route incoming packets on port 80 to the transparent proxy
iptables -A PREROUTING -t nat -p tcp --dport 80 -i ens5 -j REDIRECT --to-port 1200
# route incoming packets on port 443 to the transparent proxy
iptables -A PREROUTING -t nat -p tcp --dport 443 -i ens5 -j REDIRECT --to-port 1200
# route incoming packets on port 1025:65535 to the transparent proxy
iptables -A PREROUTING -t nat -p tcp --dport 1025:65535 -i ens5 -j REDIRECT --to-port 1200
```

```bash
wget -O port-to-vsock-transparent http://public.artifacts.marlin.pro/projects/enclaves/port-to-vsock-transparent_v1.0.0_linux_amd64
chmod +x port-to-vsock-transparent
./port-to-vsock-transparent --vsock 88 --ip-addr 0.0.0.0:1200
```

#### Build and run enclave

The following steps should be run in AWS Nitro-based instances.

https://docs.aws.amazon.com/enclaves/latest/user/getting-started.html

```bash
sudo nitro-cli build-enclave --docker-uri ghcr.io/ldclabs/idempotent-proxy_enclave_amd64:latest --output-file idempotent-proxy_enclave_amd64.eif
# Start building the Enclave Image...
# Using the locally available Docker image...
# Enclave Image successfully created.
# {
#   "Measurements": {
#     "HashAlgorithm": "Sha384 { ... }",
#     "PCR0": "bbfe317cdaba604e1364fbd254150ce25516d83e31a87f8b3d8acb163286f57f51d8b3f6b2a482ac209b758334d996d9",
#     "PCR1": "4b4d5b3661b3efc12920900c80e126e4ce783c522de6c02a2a5bf7af3a2b9327b86776f188e4be1c1c404a129dbda493",
#     "PCR2": "9ea2080d6e6bd61f03a62357a1cbbae278b070db5df6b1fe5c57821ff249b77add0f95dab0a5beec7aa6ef6735f27b14"
#   }
# }
sudo nitro-cli run-enclave --cpu-count 2 --memory 512 --enclave-cid 88 --eif-path idempotent-proxy_enclave_amd64.eif --debug-mode
# Started enclave with enclave-cid: 88, memory: 512 MiB, cpu-ids: [1, 3]
# {
#   "EnclaveName": "idempotent-proxy_enclave_amd64",
#   "EnclaveID": "i-056e1ab9a31cd77a0-enc190ca7263013fd3",
#   "ProcessID": 21493,
#   "EnclaveCID": 88,
#   "NumberOfCPUs": 2,
#   "CPUIDs": [
#     1,
#     3
#   ],
#   "MemoryMiB": 512
# }
sudo nitro-cli describe-enclaves
sudo nitro-cli console --enclave-id i-056e1ab9a31cd77a0-enc190ca7263013fd3
sudo nitro-cli terminate-enclave --enclave-id i-056e1ab9a31cd77a0-enc190ca7263013fd3
```


#### Make a request

```bash
curl -v -X POST \
  --url http://YOUR_HOST/ \
  --header 'content-type: application/json' \
  --header 'x-forwarded-host: cloudflare-eth.com' \
  --header 'idempotency-key: key_001' \
  --data '{
	  "id": 1,
    "jsonrpc": "2.0",
    "method": "eth_getBlockByNumber",
    "params": ["latest", false]
}'
```

## License
Copyright © 2024 [LDC Labs](https://github.com/ldclabs).

`ldclabs/idempotent-proxy` is licensed under the MIT License. See [LICENSE](../../LICENSE-MIT) for the full license text.