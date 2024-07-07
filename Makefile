# options
ignore_output = &> /dev/null

.PHONY: run-dev test lint fix

run-dev:
	@cargo run

test:
	@cargo test --workspace -- --nocapture

test-all:
	@cargo test --workspace -- --nocapture --include-ignored

lint:
	@cargo clippy --all-targets --all-features --workspace --tests

fix:
	@cargo clippy --fix --workspace --tests

build:
	@DOCKER_BUILDKIT=1 docker build -f Dockerfile -t ldclabs/idempotent-proxy:latest .

build-linux:
	@DOCKER_BUILDKIT=1 docker build --output target -f linux.Dockerfile .

# cargo install ic-wasm
build-wasm:
	@cargo build --release --target wasm32-unknown-unknown --package idempotent-proxy-canister
	@cargo build --release --target wasm32-unknown-unknown --package eth-canister

# cargo install candid-extractor
build-did:
	@candid-extractor target/wasm32-unknown-unknown/release/idempotent_proxy_canister.wasm > src/idempotent-proxy-canister/idempotent-proxy-canister.did
	@candid-extractor target/wasm32-unknown-unknown/release/eth_canister.wasm > examples/eth-canister/eth-canister.did
