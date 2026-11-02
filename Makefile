.PHONY: help run check build windows-exe windows-zip app dmg release clean

# Optional: TARGET=aarch64-apple-darwin or x86_64-apple-darwin.
# Native HEIC libraries must include the selected architecture.
PYTHON ?= python3
MSYS_PREFIX ?= C:/msys64/mingw64

help:
	@echo "make run      Run the desktop app"
	@echo "make check    Run formatting, Rust tests, lint, and packaging tests"
	@echo "make build    Build the release executable"
	@echo "make windows-exe  Build the Windows x64 executable (requires Windows MSVC tools)"
	@echo "make windows-zip  Build a Windows portable ZIP including HEIC support (on Windows)"
	@echo "make app      Build dist/Photo Convr.app (macOS)"
	@echo "make dmg      Build a local DMG with the app and Applications shortcut"
	@echo "make release  Sign and notarize a DMG (CODE_SIGN_IDENTITY + NOTARY_PROFILE required)"
	@echo "make clean    Remove build and release artifacts"

run:
	cargo run --release --locked

check:
	cargo fmt --check
	cargo test --locked
	cargo clippy --locked --all-targets -- -D warnings
	$(PYTHON) -m unittest discover -s scripts -p 'test_*.py'

build:
	cargo build --release --locked $(if $(TARGET),--target $(TARGET),)

windows-exe:
	cargo build --release --locked --target x86_64-pc-windows-msvc

windows-zip:
	$(PYTHON) scripts/package_windows.py --msys-prefix "$(MSYS_PREFIX)"

app:
	$(PYTHON) scripts/package_macos.py app

dmg:
	$(PYTHON) scripts/package_macos.py dmg

release:
	$(PYTHON) scripts/package_macos.py release

clean:
	cargo clean
	$(PYTHON) -c 'import shutil; shutil.rmtree("dist", ignore_errors=True)'
