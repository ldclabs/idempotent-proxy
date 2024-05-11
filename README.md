# Idempotent Proxy
Reverse proxy server with build-in idempotency support written in Rust.

## Overview

The idempotent-proxy is a reverse proxy service written in Rust with built-in idempotency support.
When multiple requests with the same idempotency-key arrive within a specific timeframe, only the first request is forwarded to the target service. The response is cached in Redis, and subsequent requests poll Redis to retrieve and return the first request's response.
This service can be used to proxy HTTPS Outcalls for ICP canisters, enabling integration with any Web2 service. It supports returning only the necessary headers and, for JSON data, allows response filtering based on JSON Mask to return only required fields, thus saving cycles consumption in ICP canisters.

![Idempotent Proxy](./idempotent-proxy.png)

## Todo List
- [x] Reverse proxy with build-in idempotency support
- [x] JSON response filtering
- [x] Access control
- [ ] Headers filtering
- [x] HTTPS support
- [ ] Documentation
- [ ] Docker image
- [ ] Examples with ICP canisters

## Run proxy in development mode

Run proxy:
```bash
docker run --name redis -d -p 6379:6379 redis:latest
cargo run
```

Make a request:
```bash
curl -v -X GET 'http://localhost:8080/get' \
  -H 'x-forwarded-host: httpbin.org' \
  -H 'Idempotency-Key: idempotency_key_001' \
  -H 'Content-Type: application/json'
```

Response:
```text
< HTTP/1.1 200 OK
< date: Sat, 11 May 2024 02:58:52 GMT
< content-type: application/json
< content-length: 375
< server: gunicorn/19.9.0
< access-control-allow-origin: *
< access-control-allow-credentials: true
<
{
  "args": {},
  "headers": {
    "Accept": "*/*",
    "Accept-Encoding": "gzip",
    "Content-Type": "application/json",
    "Host": "httpbin.org",
    "Idempotency-Key": "idempotency_key_001",
    "User-Agent": "curl/8.4.0",
    "X-Amzn-Trace-Id": "Root=1-663edeec-3ecfcdd35d0cf11269ca947e"
  },
  "origin": "120.204.63.236",
  "url": "https://httpbin.org/get"
}
```

Request again with the same idempotency key will return the same response.