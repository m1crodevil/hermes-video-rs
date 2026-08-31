BIN := target/release/watch2
INSTALL_DIR ?= $(HOME)/.local/bin

.PHONY: check build install

check:
	cargo fmt --check
	cargo test --all-targets
	cargo clippy --all-targets -- -D warnings

build:
	cargo build --release

install: build
	install -Dm755 $(BIN) $(INSTALL_DIR)/watch2
	$(INSTALL_DIR)/watch2 --version
