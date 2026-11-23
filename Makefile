# Makefile for common development tasks

CONTRACTS_DIR := dupdub_contracts
WASM_DIR      := $(CONTRACTS_DIR)/target/wasm32v1-none/release
WASM_SIZE_LIMIT := 65536  # 64 KB
WASM_SIZE_WARN  := 52428  # 80% of 64 KB
WASM_SIZE_REPORT := $(CONTRACTS_DIR)/wasm-size-report.md
WASM_SIZE_CHECK := scripts/check-wasm-size.sh

# Per-contract coverage threshold (percent). CI fails if any contract falls below.
COVERAGE_THRESHOLD ?= 80
COVERAGE_REPORT := $(CONTRACTS_DIR)/coverage-report.md

# Contracts that have per-contract test/build targets.
CONTRACTS := admin_auth admin_timelock batch_payments escrow fee_calculator fee_distributor liquidity_router registry

.PHONY: all build build-optimised check-wasm-size wasm-size-report test integration-test verify-deployment lint lint-clippy lint-fmt lint-audit \
        coverage coverage-report \
        $(addprefix build-,$(CONTRACTS)) $(addprefix test-,$(CONTRACTS)) \
        deploy-testnet local-testnet-start local-testnet-stop local-testnet-deploy

all: build

help:
	@echo "Available targets:"
	@echo "  make build              Build all contracts"
	@echo "  make test               Run all contract tests"
	@echo "  make clean              Remove all build artifacts"
	@echo "  make check-wasm-size    Enforce 64KB WASM size budget on all contracts"
	@echo "  make coverage           Run per-contract coverage and enforce threshold"
	@echo "  make help               Show this help message"
	@echo "  make deploy-testnet     Build and deploy all contracts to Stellar testnet"
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
	cd $(CONTRACTS_DIR) && cargo build --target wasm32v1-none --release -p payment-escrow
	cargo test --workspace

clean:
	cargo clean
	rm -rf target
	rm -rf test_snapshots

$(addprefix build-,$(CONTRACTS)): build-%:
	cargo build -p $*

$(addprefix test-,$(CONTRACTS)): test-%:
	cargo test -p $*

# Run tarpaulin per contract (not workspace-wide) so a well-tested contract
# cannot mask a poorly tested one. Fails if any contract is below
# COVERAGE_THRESHOLD. Writes a per-contract breakdown to COVERAGE_REPORT.
coverage:
	@echo "Running per-contract coverage (threshold: $(COVERAGE_THRESHOLD)%)"
	@printf '| Contract | Coverage | Status |\n' > $(COVERAGE_REPORT); \
	printf '| --- | --- | --- |\n' >> $(COVERAGE_REPORT); \
	failed=0; \
	for contract in $(CONTRACTS); do \
		echo "==> coverage: $$contract"; \
		out=$$(cargo tarpaulin -p $$contract --out Stdout 2>&1) || { \
			echo "ERROR: tarpaulin failed for $$contract" >&2; \
			echo "$$out" >&2; \
			printf '| %s | n/a | <span style="color:red">error</span> |\n' "$$contract" >> $(COVERAGE_REPORT); \
			failed=1; continue; \
		}; \
		pct=$$(printf '%s\n' "$$out" | grep -oE '[0-9]+(\.[0-9]+)?%' | tail -n1 | tr -d '%'); \
		if [ -z "$$pct" ]; then \
			echo "ERROR: could not parse coverage for $$contract" >&2; \
			printf '| %s | n/a | <span style="color:red">unknown</span> |\n' "$$contract" >> $(COVERAGE_REPORT); \
			failed=1; continue; \
		fi; \
		if awk "BEGIN { exit !($$pct < $(COVERAGE_THRESHOLD)) }"; then \
			echo "FAIL: $$contract coverage $$pct% < $(COVERAGE_THRESHOLD)%" >&2; \
			printf '| %s | %s%% | <span style="color:red">below threshold</span> |\n' "$$contract" "$$pct" >> $(COVERAGE_REPORT); \
			failed=1; \
		else \
			echo "OK: $$contract coverage $$pct%" ; \
			printf '| %s | %s%% | <span style="color:green">ok</span> |\n' "$$contract" "$$pct" >> $(COVERAGE_REPORT); \
		fi; \
	done; \
	cat $(COVERAGE_REPORT); \
	if [ $$failed -ne 0 ]; then \
		echo "ERROR: one or more contracts are below the $(COVERAGE_THRESHOLD)% coverage threshold." >&2; \
		exit 1; \
	fi; \
	echo "All contracts meet the $(COVERAGE_THRESHOLD)% coverage threshold."

coverage-report: coverage
	@echo "Coverage report written to $(COVERAGE_REPORT)"

local-testnet-start:
	docker compose up -d
	@echo "Waiting for local Soroban testnet to become healthy..."
	@until curl -sf http://localhost:8000/soroban/rpc -X POST -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' >/dev/null 2>&1; do sleep 2; done
	@echo "Local Soroban testnet is up at http://localhost:8000/soroban/rpc"

local-testnet-stop:
	docker compose down -v

local-testnet-deploy:
	./scripts/deploy-local-testnet.sh

deploy-testnet:
	./scripts/deploy-testnet.sh

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

# Enforce the per-contract WASM size budget (WASM_SIZE_LIMIT). Reports the
# size of every contract and fails if any exceeds the limit.
check-wasm-size: build-optimised
	@WASM_DIR=$(WASM_DIR) WASM_SIZE_LIMIT=$(WASM_SIZE_LIMIT) WASM_SIZE_WARN=$(WASM_SIZE_WARN) bash $(WASM_SIZE_CHECK)

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
