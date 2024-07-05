# idempotent-proxy-cf-worker
Reverse proxy server with build-in idempotency support running as a Cloudflare Worker.

## Overview

The idempotent-proxy is a reverse proxy service with built-in idempotency support that running as a Cloudflare Worker.

When multiple requests with the same idempotency-key arrive within a specific timeframe, only the first request is forwarded to the target service. The response is cached in **Durable Object**, and subsequent requests poll the Durable Object to retrieve and return the first request's response.

This service can be used to proxy [HTTPS outcalls](https://internetcomputer.org/docs/current/developer-docs/smart-contracts/advanced-features/https-outcalls/https-outcalls-overview) for [ICP canisters](https://internetcomputer.org/docs/current/developer-docs/smart-contracts/overview/introduction), enabling integration with any Web2 service. It supports returning only the necessary headers and, for JSON data, allows response filtering based on JSON Mask to return only required fields, thus saving cycles consumption in ICP canisters.

![Idempotent Proxy](./idempotent-proxy.png)

## Run proxy in local development mode

Run proxy:
```bash
npm i
npx wrangler dev
```


## Deploy to Cloudflare Worker

In order to use Durable Objects, you must switch to a paid plan.

```bash
npm i
npx wrangler deploy
```

And then update settings in the Cloudflare dashboard to use the Worker.

A online version for testing is available at:

https://idempotent-proxy-cf-worker.zensh.workers.dev

Try it out:
```
curl -v -X GET 'https://idempotent-proxy-cf-worker.zensh.workers.dev/URL_HTTPBIN' \
  -H 'idempotency-key: id_001' \
  -H 'content-type: application/json'
```

More information: https://github.com/ldclabs/idempotent-proxy

## License
Copyright © 2024 [LDC Labs](https://github.com/ldclabs).

`ldclabs/idempotent-proxy` is licensed under the MIT License. See [LICENSE](../../LICENSE-MIT) for the full license text.