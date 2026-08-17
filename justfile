# fast-observe — verification recipes

# Full native verification: tests (default + all features), clippy, fmt.
check: test test-all clippy fmt

test:
	cargo test

test-all:
	cargo test --all-features

clippy:
	cargo clippy --all-targets
	cargo clippy --all-targets --all-features

fmt:
	cargo fmt --check

# wasm32 compile check (nightly + rust-src required).
check-wasm:
	cargo check --target wasm32-unknown-unknown -Z build-std=std,panic_abort --no-default-features --features instant
	cargo check --target wasm32-unknown-unknown -Z build-std=std,panic_abort --no-default-features --features web
