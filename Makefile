all: check test
.PHONY: all

check: style lint
.PHONY: check


# Builds

build: build-rust build-maturin build-docs
.PHONY: check

build-rust:
	@cargo +stable build --all --all-features
.PHONY: build-rust

build-maturin:
	maturin develop
.PHONY: build-maturin

build-docs:
	cd docs && make clean html
.PHONY: build-docs

# Tests

test: test-rust
.PHONY: test

test-rust:
	cargo test --all-features
.PHONY: test-rust


# Style checking

style: style-rust
.PHONY: style

style-rust:
	@rustup component add rustfmt --toolchain stable 2> /dev/null
	cargo +stable fmt --all -- --check
.PHONY: style-rust


# Linting

lint: lint-rust
.PHONY: lint

lint-rust:
	@rustup component add clippy --toolchain stable 2> /dev/null
	cargo +stable clippy --all-features -- -D clippy::all
.PHONY: lint-rust


# Formatting

format: format-rust
.PHONY: format

format-rust:
	@rustup component add rustfmt --toolchain stable 2> /dev/null
	cargo +stable fmt --all
.PHONY: format-rust