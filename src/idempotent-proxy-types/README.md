# Idempotent Proxy
Reverse proxy server with build-in idempotency support written in Rust.

## Overview

The idempotent-proxy is a reverse proxy service written in Rust with built-in idempotency support.

When multiple requests with the same idempotency-key arrive within a specific timeframe, only the first request is forwarded to the target service. The response is cached in Redis, and subsequent requests poll Redis to retrieve and return the first request's response.

This service can be used to proxy [HTTPS outcalls](https://internetcomputer.org/docs/current/developer-docs/smart-contracts/advanced-features/https-outcalls/https-outcalls-overview) for [ICP canisters](https://internetcomputer.org/docs/current/developer-docs/smart-contracts/overview/introduction), enabling integration with any Web2 http service. It supports hiding secret information, access control, returning only the necessary headers and, for JSON or CBOR data, allows response filtering based on JSON Mask to return only required fields, thus saving cycles consumption in ICP canisters.

![Idempotent Proxy](./idempotent-proxy.png)

## Features
- [x] Reverse proxy with build-in idempotency support
- [x] JSON response filtering
- [x] Access control
- [x] Response headers filtering
- [x] HTTPS support
- [x] Running as Cloudflare Worker
- [x] Docker image

More information: https://github.com/ldclabs/idempotent-proxy

## License
Copyright © 2024 [LDC Labs](https://github.com/ldclabs).

`ldclabs/idempotent-proxy` is licensed under the MIT License. See [LICENSE](LICENSE-MIT) for the full license text.