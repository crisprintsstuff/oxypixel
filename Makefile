.PHONY: test check check-windows release windows installer fmt

test:
	cargo test --workspace

fmt:
	cargo fmt --all

check:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

# Clippy for #[cfg(windows)] code from Linux.
# Needs the MinGW compiler x86_64-w64-mingw32-gcc
# (Debian/Ubuntu package gcc-mingw-w64-x86-64) and the rustup target
# x86_64-pc-windows-gnu. The MSVC target is what CI and the release build
# use. This target type-checks the Windows cfg; it does not run those tests.
MINGW_CC ?= x86_64-w64-mingw32-gcc
WINDOWS_GNU_TARGET ?= x86_64-pc-windows-gnu

check-windows:
	@if ! command -v $(MINGW_CC) >/dev/null 2>&1; then \
		printf '%s\n' \
			"check-windows: $(MINGW_CC) is not on PATH." \
			"Install the MinGW cross compiler. On Debian or Ubuntu: sudo apt-get install gcc-mingw-w64-x86-64" >&2; \
		exit 1; \
	fi
	@if ! command -v rustup >/dev/null 2>&1; then \
		printf '%s\n' \
			"check-windows: rustup is not on PATH." \
			"Install Rust with rustup, then run: rustup target add $(WINDOWS_GNU_TARGET)" >&2; \
		exit 1; \
	fi
	@if ! rustup target list --installed | grep -qx '$(WINDOWS_GNU_TARGET)'; then \
		printf '%s\n' \
			"check-windows: rustup target $(WINDOWS_GNU_TARGET) is not installed." \
			"Run: rustup target add $(WINDOWS_GNU_TARGET)" >&2; \
		exit 1; \
	fi
	cargo clippy --workspace --all-targets --target $(WINDOWS_GNU_TARGET) -- -D warnings

release:
	cargo build --workspace --release

windows:
	./scripts/build-windows.sh

installer:
	./scripts/build-installer.sh
