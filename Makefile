.PHONY: build test test-e2e lint fmt sync clean

build:
	cargo build --workspace

# Unit + integration tests, then the cucumber skip check (needs its own env).
test:
	cargo test --workspace
	SKIPPER_E2E=1 cargo test -p skipper-cucumber --test bdd

lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings

fmt:
	cargo fmt --all

# Dogfooding: run the suite through cargo-skipper and reconcile the skipper-rust sheet.
sync:
	SKIPPER_MODE=sync SKIPPER_SHEET_NAME=skipper-rust cargo run -q -p cargo-skipper -- skipper test --workspace

clean:
	cargo clean
