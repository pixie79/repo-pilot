PREFIX ?= $(HOME)/.local

.PHONY: install
install:
	cargo install --force --path crates/repo-pilot --root $(PREFIX)

.PHONY: check
check:
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings
	cargo test
