# Idempotent Proxy
Reverse proxy server with build-in idempotency support written in Rust.

## Overview

The idempotent-proxy is a reverse proxy service written in Rust with built-in idempotency support.

When multiple requests with the same idempotency-key arrive within a specific timeframe, only the first request is forwarded to the target service. The response is cached in Redis, and subsequent requests poll Redis to retrieve and return the first request's response.

This service can be used to proxy [HTTPS outcalls](https://internetcomputer.org/docs/current/developer-docs/smart-contracts/advanced-features/https-outcalls/https-outcalls-overview) for [ICP canisters](https://internetcomputer.org/docs/current/developer-docs/smart-contracts/overview/introduction), enabling integration with any Web2 service. It supports returning only the necessary headers and, for JSON data, allows response filtering based on JSON Mask to return only required fields, thus saving cycles consumption in ICP canisters.

![Idempotent Proxy](./idempotent-proxy.png)

If you plan to use this project and have any questions, feel free to open an issue. I will address it as soon as possible.

## Todo List
- [x] Reverse proxy with build-in idempotency support
- [x] JSON response filtering
- [x] Access control
- [x] Response headers filtering
- [x] HTTPS support
- [x] Documentation
- [ ] Docker image
- [ ] Examples with ICP canisters

## Run proxy in development mode

Run proxy:
```bash
docker run --name redis -d -p 6379:6379 redis:latest
cargo run
```

### Regular Proxy Request Example

Make a request:
```bash
curl -v -X GET 'http://localhost:8080/get' \
  -H 'x-forwarded-host: httpbin.org' \
  -H 'idempotency-key: idempotency_key_001' \
  -H 'content-type: application/json'
```

Response:
```text
< HTTP/1.1 200 OK
< date: Wed, 22 May 2024 11:03:33 GMT
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
    "User-Agent": "curl/8.6.0",
    "X-Amzn-Trace-Id": "Root=1-664dd105-7930bcc43ae6081a4508d114"
  },
  "origin": "120.204.60.218",
  "url": "https://httpbin.org/get"
}
```

Request again with the same idempotency key will return the same response.

### Proxy Request Example with `URL_` Constant Defined

Setting in .env file:
```text
URL_HTTPBIN="https://httpbin.org/get?api-key=abc123"
```

Make a request with `URL_HTTPBIN` constant in url path:
```bash
curl -v -X GET 'http://localhost:8080/URL_HTTPBIN' \
  -H 'idempotency-key: idempotency_key_001' \
  -H 'content-type: application/json'
```

Response:
```text
< HTTP/1.1 200 OK
< date: Wed, 22 May 2024 11:07:05 GMT
< content-type: application/json
< content-length: 417
< server: gunicorn/19.9.0
< access-control-allow-origin: *
< access-control-allow-credentials: true
<
{
  "args": {
    "api-key": "abc123"
  },
  "headers": {
    "Accept": "*/*",
    "Accept-Encoding": "gzip",
    "Content-Type": "application/json",
    "Host": "httpbin.org",
    "Idempotency-Key": "idempotency_key_001",
    "User-Agent": "curl/8.6.0",
    "X-Amzn-Trace-Id": "Root=1-664dd1d9-6612bfd076e95b814dd9329d"
  },
  "origin": "120.204.60.218",
  "url": "https://httpbin.org/get?api-key=abc123"
}
```

### Proxy Request Example with `HEADER_` Constant Defined

Setting in .env file:
```text
URL_HTTPBIN="https://httpbin.org/get?api-key=abc123"
HEADER_TOKEN="Bearer xyz123456"
```

Make a request with `HEADER_TOKEN` constant in header:
```bash
curl -v -X GET 'http://localhost:8080/URL_HTTPBIN' \
  -H 'idempotency-key: idempotency_key_001' \
  -H 'authorization: HEADER_TOKEN' \
  -H 'content-type: application/json'
```

Response:
```text
< HTTP/1.1 200 OK
< date: Wed, 22 May 2024 11:11:17 GMT
< content-type: application/json
< content-length: 459
< server: gunicorn/19.9.0
< access-control-allow-origin: *
< access-control-allow-credentials: true
<
{
  "args": {
    "api-key": "abc123"
  },
  "headers": {
    "Accept": "*/*",
    "Accept-Encoding": "gzip",
    "Authorization": "Bearer xyz123456",
    "Content-Type": "application/json",
    "Host": "httpbin.org",
    "Idempotency-Key": "idempotency_key_001",
    "User-Agent": "curl/8.6.0",
    "X-Amzn-Trace-Id": "Root=1-664dd2d5-15b233f974a01ca34bd9a8ab"
  },
  "origin": "120.204.60.218",
  "url": "https://httpbin.org/get?api-key=abc123"
}
```

### Proxy Request Example with Response Headers Filtered

Make a request with `response-headers` header:
```bash
curl -v -X GET 'http://localhost:8080/URL_HTTPBIN' \
  -H 'idempotency-key: idempotency_key_001' \
  -H 'authorization: HEADER_TOKEN' \
  -H 'response-headers: content-type,content-length' \
  -H 'content-type: application/json'
```

Response:
```text
< HTTP/1.1 200 OK
< content-type: application/json
< content-length: 515
< date: Wed, 22 May 2024 11:13:39 GMT
<
{
  "args": {
    "api-key": "abc123"
  },
  "headers": {
    "Accept": "*/*",
    "Accept-Encoding": "gzip",
    "Authorization": "Bearer xyz123456",
    "Content-Type": "application/json",
    "Host": "httpbin.org",
    "Idempotency-Key": "idempotency_key_001",
    "Response-Headers": "content-type,content-length",
    "User-Agent": "curl/8.6.0",
    "X-Amzn-Trace-Id": "Root=1-664dd363-2bbae4420bf9add8512f5930"
  },
  "origin": "120.204.60.218",
  "url": "https://httpbin.org/get?api-key=abc123"
}
```

### Proxy Request Example with JSON Response Filtered

Make a request with `x-json-mask` header:
```bash
curl -v -X GET 'http://localhost:8080/URL_HTTPBIN' \
  -H 'idempotency-key: idempotency_key_001' \
  -H 'authorization: HEADER_TOKEN' \
  -H 'response-headers: content-type,content-length' \
  -H 'x-json-mask: args,url' \
  -H 'content-type: application/json'
```

Response:
```text
< HTTP/1.1 200 OK
< content-type: application/json
< content-length: 76
< date: Wed, 22 May 2024 12:19:03 GMT
<
* Connection #0 to host localhost left intact
{"args":{"api-key":"abc123"},"url":"https://httpbin.org/get?api-key=abc123"}
```

### Proxy Request Example with Access Control Added

Setting in .env file:
```text
ECDSA_PUB_KEY_1="A6t1U8kc10AbLJ3-V1avU4rYvmAsYjXuzY0kPublttot"
```

You can add other public keys by adding `ECDSA_PUB_KEY_2`, `ECDSA_PUB_KEY_abc` for key rotation.

Make a request with `proxy-authorization` header, the bearer token is signed with the private key:
```bash
curl -v -X GET 'http://localhost:8080/URL_HTTPBIN' \
  -H 'idempotency-key: idempotency_key_001' \
  -H 'proxy-authorization: Bearer 6LduPbIpAAAAANSOUfb-8bU45eilZFSmlSguN5TO' \
  -H 'authorization: HEADER_TOKEN' \
  -H 'response-headers: content-type,content-length' \
  -H 'x-json-mask: args,url' \
  -H 'content-type: application/json'
```

A 407 response:
```text
< HTTP/1.1 407 Proxy Authentication Required
< content-type: text/plain; charset=utf-8
< content-length: 34
< date: Wed, 22 May 2024 12:24:40 GMT
<
* Connection #0 to host localhost left intact
proxy authentication verify failed: failed to decode CBOR data
```