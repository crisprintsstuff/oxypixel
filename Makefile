.PHONY: test check release windows installer fmt

test:
	cargo test --workspace

fmt:
	cargo fmt --all

check:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

release:
	cargo build --workspace --release

windows:
	./scripts/build-windows.sh

installer:
	./scripts/build-installer.sh
