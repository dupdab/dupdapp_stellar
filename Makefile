# Makefile for common development tasks

CONTRACTS_DIR := dupdub_contracts
WASM_DIR      := $(CONTRACTS_DIR)/target/wasm32v1-none/release
WASM_SIZE_LIMIT := 65536  # 64 KB
WASM_SIZE_WARN  := 52428  # 80% of 64 KB
WASM_SIZE_REPORT := $(CONTRACTS_DIR)/wasm-size-report.md

# Contracts that have per-contract test/build targets.
CONTRACTS := escrow registry

.PHONY: build build-optimised check-wasm-size wasm-size-report test integration-test verify-deployment lint lint-clippy lint-fmt lint-audit \
        $(addprefix build-,$(CONTRACTS)) $(addprefix test-,$(CONTRACTS)) \
        local-testnet-start local-testnet-stop local-testnet-deploy

# Run all lint checks in sequence. Stops on the first failure with a clear
# message identifying which check failed.
lint:
	@echo "==> [1/3] Running clippy..."
	@$(MAKE) --no-print-directory lint-clippy || { echo "ERROR: clippy check failed. Run 'cargo clippy --all-targets --all-features -- -D warnings' to see details." >&2; exit 1; }
	@echo "==> [2/3] Running fmt check..."
	@$(MAKE) --no-print-directory lint-fmt || { echo "ERROR: fmt check failed. Run 'cargo fmt --all' to fix formatting." >&2; exit 1; }
	@echo "==> [3/3] Running audit..."
	@$(MAKE) --no-print-directory lint-audit || { echo "ERROR: audit check failed. Run 'cargo audit' to see details." >&2; exit 1; }
	@echo "==> All lint checks passed."

# Per-contract build: `make build-<contract>` builds a single contract.
$(addprefix build-,$(CONTRACTS)): build-%:
	cd $(CONTRACTS_DIR) && cargo build --target wasm32v1-none --release -p $*

build-optimised: build
	@echo "Running wasm-opt -Oz on all contract WASMs..."
	@for wasm in $(WASM_DIR)/*.wasm; do \
		echo "  optimising $$wasm"; \
		wasm-opt -Oz --output $$wasm $$wasm; \
	done
	@echo "Done."

lint-clippy:
	cargo clippy --all-targets --all-features -- -D warnings

lint-fmt:
	cargo fmt --all -- --check

lint-audit:
	cargo audit

# Emit a Markdown table of contract WASM sizes for posting to a PR comment.
# Columns: contract | current size | previous size | delta.
# Contracts over 80% of the budget are highlighted yellow; over budget red.
# Set PREVIOUS_WASM_DIR to a directory of baseline WASMs to compute deltas.
wasm-size-report: build-optimised
	@echo "Generating WASM size report -> $(WASM_SIZE_REPORT)"
	@printf '| Contract | Current size | Previous size | Delta |\n' > $(WASM_SIZE_REPORT); \
	printf '| --- | --- | --- | --- |\n' >> $(WASM_SIZE_REPORT); \
	for wasm in $(WASM_DIR)/*.wasm; do \
		name=$$(basename $$wasm); \
		size=$$(wc -c < $$wasm); \
		prev="n/a"; \
		delta="n/a"; \
		if [ -n "$(PREVIOUS_WASM_DIR)" ] && [ -f "$(PREVIOUS_WASM_DIR)/$$name" ]; then \
			prev=$$(wc -c < "$(PREVIOUS_WASM_DIR)/$$name"); \
			diff=$$((size - prev)); \
			if [ $$diff -gt 0 ]; then delta="+$$diff"; else delta="$$diff"; fi; \
		fi; \
		if [ $$size -gt $(WASM_SIZE_LIMIT) ]; then \
			printf '| <span style="color:red">%s</span> | <span style="color:red">%s bytes</span> | %s | %s |\n' "$$name" "$$size" "$$prev" "$$delta" >> $(WASM_SIZE_REPORT); \
		elif [ $$size -gt $(WASM_SIZE_WARN) ]; then \
			printf '| <span style="color:orange">%s</span> | <span style="color:orange">%s bytes</span> | %s | %s |\n' "$$name" "$$size" "$$prev" "$$delta" >> $(WASM_SIZE_REPORT); \
		else \
			printf '| %s | %s bytes | %s | %s |\n' "$$name" "$$size" "$$prev" "$$delta" >> $(WASM_SIZE_REPORT); \
		fi; \
	done; \
	cat $(WASM_SIZE_REPORT)

test:
	cd $(CONTRACTS_DIR) && cargo test

# Per-contract test: `make test-<contract>` tests a single contract.
$(addprefix test-,$(CONTRACTS)): test-%:
	cd $(CONTRACTS_DIR) && cargo test -p $*

# Start a local Soroban testnet via stellar/quickstart (docker compose).
local-testnet-start:
	docker compose up -d stellar-quickstart
	@echo "Local Soroban testnet starting. RPC: http://localhost:8000/soroban/rpc"

# Stop the local Soroban testnet container.
local-testnet-stop:
	docker compose down

# Build and deploy contracts against the local testnet.
local-testnet-deploy: build
	./scripts/deploy-local.sh

integration-test: build
	@echo "Running full payment lifecycle integration test against local testnet..."
	./scripts/integration-test.sh

verify-deployment:
	bash scripts/verify-deployment.sh

clean:
	cd $(CONTRACTS_DIR) && cargo clean

help:
	@echo "Available targets:"
	@echo "  build                 Build all contracts (wasm32v1-none, release)"
	@echo "  build-optimised       Build all contracts and run wasm-opt -Oz"
	@echo "  check-wasm-size       Verify all WASMs are within the size limit"
	@echo "  test                  Run the full test suite"
	@echo "  local-testnet-start   Start a local Soroban testnet (docker compose)"
	@echo "  local-testnet-stop    Stop the local Soroban testnet"
	@echo "  local-testnet-deploy  Build and deploy contracts to the local testnet"
	@echo "  integration-test      Run the full payment lifecycle integration test"
	@echo "  verify-deployment     Verify a deployment"
	@echo "  clean                 Remove all build artifacts"
	@echo "  help                  Show this help message"
	@for c in $(CONTRACTS); do \
		echo "  build-$$c             Build the $$c contract"; \
		echo "  test-$$c              Test the $$c contract"; \
	done
