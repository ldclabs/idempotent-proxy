# Idempotent Proxy
Reverse proxy server with build-in idempotency support written in Rust.

## Overview

## Run proxy

```bash
docker run --name redis -d -p 6379:6379 redis:latest
cargo run
```