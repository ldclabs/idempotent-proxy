# Idempotent Proxy
Reverse proxy server with build-in idempotency support written in Rust.

## Overview

The idempotent-proxy is a reverse proxy service written in Rust with built-in idempotency support.

When multiple requests with the same idempotency-key arrive within a specific timeframe, only the first request is forwarded to the target service. The response is cached in Redis, and subsequent requests poll Redis to retrieve and return the first request's response.

This service can be used to proxy [HTTPS outcalls](https://internetcomputer.org/docs/current/developer-docs/smart-contracts/advanced-features/https-outcalls/https-outcalls-overview) for [ICP canisters](https://internetcomputer.org/docs/current/developer-docs/smart-contracts/overview/introduction), enabling integration with any Web2 http service. It supports hiding secret information, access control, returning only the necessary headers and, for JSON or CBOR data, allows response filtering based on JSON Mask to return only required fields, thus saving cycles consumption in ICP canisters.

![Idempotent Proxy](./idempotent-proxy.png)

If you plan to use this project and have any questions, feel free to open an issue. I will address it as soon as possible.

## Features
- [x] Reverse proxy with build-in idempotency support
- [x] JSON response filtering
- [x] Access control
- [x] Response headers filtering
- [x] HTTPS support
- [x] Running as Cloudflare Worker
- [x] Docker image

## Running as Cloudflare Worker

Idempotent Proxy can be running as a Cloudflare Worker. See the [idempotent-proxy-cf-worker](./src/idempotent-proxy-cf-worker) directory for more information.

## Deploy

### Run proxy in development mode

Run proxy:
```bash
docker run --name redis -d -p 6379:6379 redis:latest
cargo run -p idempotent-proxy-server
```

### Run proxy with Docker

files in `/mnt/idempotent-proxy` directory:
```
/mnt/idempotent-proxy/.env
/mnt/idempotent-proxy/keys/doge-test-rpc.panda.fans.key
/mnt/idempotent-proxy/keys/doge-test-rpc.panda.fans.pem
```

`.env` file:
```text
SERVER_ADDR=0.0.0.0:443
REDIS_URL=172.16.32.1:6379
POLL_INTERVAL=100 # in milliseconds
REQUEST_TIMEOUT=10000 # in milliseconds
LOG_LEVEL=info # debug, info, warn, error
# cert file path to enable https, for example: /etc/https/mydomain.crt
TLS_CERT_FILE = "keys/doge-test-rpc.panda.fans.pem"
# key file path to enable https, for example: /etc/https/mydomain.key
TLS_KEY_FILE = "keys/doge-test-rpc.panda.fans.key"

ECDSA_PUB_KEY_1="A44DZpzDwDvq9HwW3_dynOfDgkMJHKgOxUyCOrv5Pl3O"

# ECDSA_PUB_KEY_2="xxxxxx"

ALLOW_AGENTS="ICPanda"

URL_DOGE_TEST="http://172.16.32.1:44555/"
URL_DOGE="http://172.16.32.1:22555/"
# URL_XXX=...

HEADER_API_TOKEN="Basic SUNQYW5kYTpJVEZDNlJjam56RkdEQnd0SzByYV9kS0swR29lSElqVUl3V2lEb3VrRWU0"
# HEADER_XXX=...
```

Run proxy with Docker:
```bash
docker run --restart=always -v /mnt/idempotent-proxy/.env:/app/.env -v /mnt/idempotent-proxy/keys:/app/keys --name proxy -d -p 443:443 ghcr.io/ldclabs/idempotent-proxy:latest
```

### Deploy to Cloudflare Worker

In order to use Durable Objects, you must switch to a paid plan.

```bash
cd src/idempotent-proxy-cf-worker
npm i
npx wrangler deploy
```

And then update settings in the Cloudflare dashboard to use the Worker.

### Online version for testing

A online version for testing is available at:

https://idempotent-proxy-cf-worker.zensh.workers.dev

Try it out:
```
curl -v -X GET 'https://idempotent-proxy-cf-worker.zensh.workers.dev/URL_HTTPBIN' \
  -H 'idempotency-key: idempotency_key_001' \
  -H 'content-type: application/json'
```

## Request Examples

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

## License
Copyright © 2024 [LDC Labs](https://github.com/ldclabs).

`ldclabs/idempotent-proxy` is licensed under the MIT License. See [LICENSE](LICENSE-MIT) for the full license text.