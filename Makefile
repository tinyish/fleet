# The commands used every day (fleet-onbm). Each target only runs what
# cargo or tools/ already do; `make` alone lists them.
#
# The gates (core.md § Toolchain) run twice, without and with
# --all-features, so the CLI's specimen builds and stays complete. Builds
# with features go to target/features, so target/debug/fleet, which a real
# supervisor may run from, stays a plain build.

CARGO ?= cargo
FEATURES := CARGO_TARGET_DIR=target/features $(CARGO)

.DEFAULT_GOAL := help
.PHONY: help build release test lint fmt fmt-check check specimen try clean

help: ## List the targets
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F ':.*## ' '{printf "  make %-10s %s\n", $$1, $$2}'

build: ## Build fleet (target/debug/fleet)
	$(CARGO) build -p fleet

release: ## Build fleet as a release would (target/release/fleet)
	$(CARGO) build --release --locked -p fleet

test: ## Run the tests, without and with --all-features
	$(CARGO) test --workspace
	$(FEATURES) test --workspace --all-features

lint: ## Run clippy, without and with --all-features
	$(CARGO) clippy --workspace --all-targets
	$(FEATURES) clippy --workspace --all-targets --all-features

fmt: ## Format the code
	$(CARGO) fmt --all

fmt-check: ## Check the formatting, changing nothing
	$(CARGO) fmt --all --check

check: fmt-check lint test ## Every gate a piece must pass

specimen: ## Print every line the CLI can print
	$(FEATURES) run -q -p fleet --features specimen -- specimen

try: ## A sandbox shell to try fleet by hand (tools/try)
	tools/try

clean: ## Remove the build output, the sandbox build included
	$(CARGO) clean
