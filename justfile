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

# wasm32-wasip3 compile check (PRIMARY wasm target, DESIGN.md §11b).
# Stock builtin tier-3 target — no custom target json needed for `check`
# (no link step). Runtime proof: wasmtron's crates/observe-spike.
check-wasip3:
	cargo check --target wasm32-wasip3 -Z build-std=std,panic_abort --no-default-features --features instant
	cargo check --target wasm32-wasip3 -Z build-std=std,panic_abort --no-default-features --features backtrace
	cargo check --target wasm32-wasip3 -Z build-std=std,panic_abort --no-default-features --features fastrace
	# `web` degrades to `instant` spans only on wasip3 (browser-only cfg).
	cargo check --target wasm32-wasip3 -Z build-std=std,panic_abort --no-default-features --features web
