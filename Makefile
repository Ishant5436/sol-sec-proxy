CC = cargo
CARGO_FLAGS = --release

.PHONY: all build test lint check run clean audit

all: check test build

build:
	$(CC) build $(CARGO_FLAGS)

test:
	$(CC) test -- --nocapture

lint:
	$(CC) clippy -- -D warnings
	$(CC) fmt --check

format:
	$(CC) fmt

check:
	$(CC) check

run:
	$(CC) run --bin sol-sec-proxy

clean:
	$(CC) clean

audit:
	@echo "Auditing deterministic safety standards..."
	@which cargo-audit >/dev/null 2>&1 && cargo audit || echo "cargo-audit not installed, skipping advisories"
