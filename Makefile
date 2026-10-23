# Makefile for common development tasks

CONTRACTS_DIR := dupdub_contracts
WASM_DIR      := $(CONTRACTS_DIR)/target/wasm32v1-none/release
WASM_SIZE_LIMIT := 65536  # 64 KB
WASM_SIZE_WARN  := 52428  # 80% of 64 KB
WASM_SIZE_REPORT := $(CONTRACTS_DIR)/wasm-size-report.md

# Contracts that have per-contract test/build targets.
CONTRACTS := admin_auth admin_timelock batch_payments escrow fee_calculator fee_distributor liquidity_router registry

.PHONY: all build build-optimised check-wasm-size wasm-size-report test integration-test verify-deployment lint lint-clippy lint-fmt lint-audit \
        $(addprefix build-,$(CONTRACTS)) $(addprefix test-,$(CONTRACTS)) \
        local-testnet-start local-testnet-stop local-testnet-deploy

all: build

help:
	@echo "Available targets:"
	@echo "  make build              Build all contracts"
	@echo "  make test               Run all contract tests"
	@echo "  make clean              Remove all build artifacts"
	@echo "  make help               Show this help message"
	@echo ""
	@echo "Local Soroban testnet targets:"
	@echo "  make local-testnet-start   Start local Soroban testnet (Docker)"
	@echo "  make local-testnet-stop    Stop local Soroban testnet"
	@echo "  make local-testnet-deploy  Deploy contracts to local testnet"
	@echo ""
	@echo "Per-contract build targets:"
	@$(foreach c,$(CONTRACTS),echo "  make build-$(c)";)
	@echo ""
	@echo "Per-contract test targets:"
	@$(foreach c,$(CONTRACTS),echo "  make test-$(c)";)

build:
	cargo build --workspace

test:
	cargo test --workspace

clean:
	cargo clean
	rm -rf target
	rm -rf test_snapshots

$(addprefix build-,$(CONTRACTS)): build-%:
	cargo build -p $*

$(addprefix test-,$(CONTRACTS)): test-%:
	cargo test -p $*

local-testnet-start:
	docker compose up -d
	@echo "Waiting for local Soroban testnet to become healthy..."
	@until curl -sf http://localhost:8000/soroban/rpc -X POST -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' >/dev/null 2>&1; do sleep 2; done
	@echo "Local Soroban testnet is up at http://localhost:8000/soroban/rpc"

local-testnet-stop:
	docker compose down -v

local-testnet-deploy:
	./scripts/deploy-local-testnet.sh

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

bindings: build
	@echo "Generating TypeScript bindings for all contracts..."
	@mkdir -p $(BINDINGS_DIR)
	@for contract in $(CONTRACTS); do \
		wasm=$(WASM_DIR)/$$contract.wasm; \
		if [ ! -f $$wasm ]; then \
			echo "  SKIP  $$contract (missing $$wasm)"; \
			continue; \
		fi; \
		echo "  generating $$contract"; \
		stellar contract bindings typescript \
			--wasm $$wasm \
			--output-dir $(BINDINGS_DIR)/$$contract \
			--overwrite || exit 1; \
	done
	@echo "Bindings written to $(BINDINGS_DIR)."

integration-test: build
	@echo "Running full payment lifecycle integration test against local testnet..."
	./scripts/integration-test.sh

verify-deployment:
	bash scripts/verify-deployment.sh

