# Makefile for common development tasks

CONTRACTS_DIR := dupdub_contracts
WASM_DIR      := $(CONTRACTS_DIR)/target/wasm32v1-none/release
WASM_SIZE_LIMIT := 65536  # 64 KB
WASM_SIZE_WARN  := 49152  # 48 KB (75% of 64 KB) early-warning threshold
WASM_SIZE_REPORT := $(CONTRACTS_DIR)/wasm-size-report.md
WASM_SIZE_CHECK := scripts/check-wasm-size.sh

# wasm-opt optimization level. -Oz minimizes code size (lower ledger rent,
# more headroom under the 64 KB budget) at a small execution-cost premium;
# -O3 favors execution speed at the cost of larger WASMs. See README for the
# Oz vs O3 benchmark rationale. Override with `make build-optimised WASM_OPT_LEVEL=-O3`.
WASM_OPT_LEVEL ?= -Oz

# Per-contract coverage threshold (percent). CI fails if any contract falls below.
COVERAGE_THRESHOLD ?= 80
COVERAGE_REPORT := $(CONTRACTS_DIR)/coverage-report.md
COVERAGE_DIR       := $(CONTRACTS_DIR)/coverage

# Contracts that have per-contract test/build targets.
CONTRACTS := admin_auth admin_timelock batch_payments escrow fee_calculator fee_distributor liquidity_router registry

# Per-contract WASM size budgets (bytes). Each contract must stay at or below
# its own budget; the check fails if any contract exceeds it. Budgets default
# to WASM_SIZE_LIMIT and can be tightened per contract as needed.
WASM_SIZE_BUDGET_admin_auth       ?= $(WASM_SIZE_LIMIT)
WASM_SIZE_BUDGET_admin_timelock   ?= $(WASM_SIZE_LIMIT)
WASM_SIZE_BUDGET_batch_payments   ?= $(WASM_SIZE_LIMIT)
WASM_SIZE_BUDGET_escrow           ?= $(WASM_SIZE_LIMIT)
WASM_SIZE_BUDGET_fee_calculator   ?= $(WASM_SIZE_LIMIT)
WASM_SIZE_BUDGET_fee_distributor  ?= $(WASM_SIZE_LIMIT)
WASM_SIZE_BUDGET_liquidity_router ?= $(WASM_SIZE_LIMIT)
WASM_SIZE_BUDGET_registry         ?= $(WASM_SIZE_LIMIT)

.PHONY: all help build build-optimised check-wasm-size wasm-size-report test integration-test verify-deployment lint lint-clippy lint-fmt lint-audit coverage coverage-check coverage-report \
        $(addprefix build-,$(CONTRACTS)) $(addprefix test-,$(CONTRACTS)) $(addprefix coverage-,$(CONTRACTS)) \
        deploy-testnet local-testnet-start local-testnet-stop local-testnet-deploy \
        dev dev-down simnet-start simnet-stop simnet-deploy simnet-fund \
        monitoring monitoring-stop

# Bare `make` shows help so available commands are discoverable.
.DEFAULT_GOAL := help

all: build

help:
	@echo "Available targets:"
	@echo "  make build              Build all contracts"
	@echo "  make test               Run all contract tests"
	@echo "  make clean              Remove all build artifacts"
	@echo "  make check-wasm-size    Enforce per-contract WASM size budgets"
	@echo "  make coverage           Run per-contract coverage and enforce threshold"
	@echo "  make help               Show this help message"
	@echo "  make deploy-testnet     Build and deploy all contracts to Stellar testnet"
	@echo ""
	@echo "Local dev environment (single command):"
	@echo "  make dev                Start local Stellar simnet + deploy contracts"
	@echo "  make dev-down           Stop the local dev environment"
	@echo ""
	@echo "Local Soroban testnet targets:"
	@echo "  make local-testnet-start   Start local Soroban testnet (Docker)"
	@echo "  make local-testnet-stop    Stop local Soroban testnet"
	@echo "  make local-testnet-deploy  Deploy contracts to local testnet"
	@echo ""
	@echo "Local Stellar simnet targets (stellar-cli):"
	@echo "  make simnet-start          Start a local Stellar simnet via stellar-cli"
	@echo "  make simnet-stop           Stop the local Stellar simnet"
	@echo "  make simnet-fund           Create and fund a local dev identity"
	@echo "  make simnet-deploy         Deploy contracts to the local simnet"
	@echo ""
	@echo "Local observability stack targets:"
	@echo "  make monitoring            Start Grafana + Loki + Promtail (Docker)"
	@echo "  make monitoring-stop       Stop the local observability stack"
	@echo ""
	@echo "Per-contract build targets:"
	@$(foreach c,$(CONTRACTS),echo "  make build-$(c)";)
	@echo ""
	@echo "Per-contract test targets:"
	@$(foreach c,$(CONTRACTS),echo "  make test-$(c)";)
	@echo ""
	@echo "Per-contract coverage targets:"
	@$(foreach c,$(CONTRACTS),echo "  make coverage-$(c)";)

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

$(addprefix coverage-,$(CONTRACTS)): coverage-%:
	@mkdir -p $(COVERAGE_DIR)
	cargo tarpaulin --package $* --out Xml --output-dir $(COVERAGE_DIR) --fail-under $(COVERAGE_THRESHOLD)

# Single command to bring up the full local dev environment: start the
# Stellar simnet container and deploy the contracts to it.
dev: local-testnet-start local-testnet-deploy
	@echo "Local dev environment ready. RPC: http://localhost:8000/soroban/rpc"

dev-down: local-testnet-stop

local-testnet-start:
	docker compose -f docker-compose.dev.yml up -d
	@echo "Waiting for local Soroban testnet to become healthy..."
	@until curl -sf http://localhost:8000/soroban/rpc -X POST -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' >/dev/null 2>&1; do sleep 2; done
	@echo "Local Soroban testnet is up at http://localhost:8000/soroban/rpc"

local-testnet-stop:
	docker compose -f docker-compose.dev.yml down -v

local-testnet-deploy:
	./scripts/deploy-local-testnet.sh

# Local Stellar simnet driven directly by stellar-cli (no Docker required).
# See README "Local Stellar simnet" for details.
simnet-start:
	stellar network start local
	@echo "Local simnet running. RPC: http://localhost:8000/soroban/rpc"

simnet-stop:
	stellar network stop local

simnet-fund:
	stellar keys generate dev --network local --fund
	@echo "Funded identity 'dev' on local simnet."

simnet-deploy: build
	@for contract in $(CONTRACTS); do \
		wasm=$(WASM_DIR)/$$contract.wasm; \
		if [ ! -f $$wasm ]; then \
			echo "  SKIP  $$contract (missing $$wasm)"; \
			continue; \
		fi; \
		echo "  deploying $$contract"; \
		stellar contract deploy \
			--wasm $$wasm \
			--source dev \
			--network local || exit 1; \
	done
	@echo "Contracts deployed to local simnet."

# Start the local observability stack (Grafana + Loki + Promtail) defined in
# grafana/docker-compose.yml. Grafana is exposed at http://localhost:3000.
monitoring:
	docker compose -f grafana/docker-compose.yml up -d
	@echo "Waiting for Grafana to become healthy..."
	@until curl -sf http://localhost:3000/api/health >/dev/null 2>&1; do sleep 2; done
	@echo "Grafana is up at http://localhost:3000 (admin/admin)"

monitoring-stop:
	docker compose -f grafana/docker-compose.yml down -v

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

# Optimize contract WASMs with wasm-opt. Default level is -Oz (size-optimized);
# see the README for the Oz vs O3 code-size/execution-cost tradeoff and the
# benchmark summary printed below.
build-optimised: build
	@echo "Running wasm-opt $(WASM_OPT_LEVEL) on all contract WASMs..."
	@for wasm in $(WASM_DIR)/*.wasm; do \
		echo "  optimising $$wasm"; \
		wasm-opt $(WASM_OPT_LEVEL) --output $$wasm $$wasm; \
	done
	@echo "Done."
	@echo ""
	@echo "wasm-opt optimization level: $(WASM_OPT_LEVEL)"
	@echo "  -Oz (default): smallest WASM, lowest ledger rent, ~5-10% higher execution cost"
	@echo "  -O3 (opt-in):  fastest execution, ~10-20% larger WASM, more rent"
	@echo "  Benchmark (escrow transfer, relative): -Oz size 1.00x / exec 1.05x; -O3 size 1.15x / exec 1.00x"
	@echo "  Rationale: 64KB budget + rent dominate; -Oz chosen. Override: make build-optimised WASM_OPT_LEVEL=-O3"
	@echo ""

# Enforce the per-contract WASM size budget. Reports the size of every
# contract against its own budget and fails (non-zero exit) if any contract
# exceeds it, so PR CI fails before a size violation reaches deploy.
# Contracts over WASM_SIZE_WARN (75% of the budget) emit an early-warning
# alert without failing the build.
check-wasm-size: build-optimised
	@status=0; \
	for contract in $(CONTRACTS); do \
		wasm=$(WASM_DIR)/$$contract.wasm; \
		budget=$$(echo "$(foreach c,$(CONTRACTS),$(c)=$(WASM_SIZE_BUDGET_$(c));)" | tr ';' '\n' | awk -F= -v c=$$contract '$$1==c{print $$2}'); \
		if [ -z "$$budget" ]; then budget=$(WASM_SIZE_LIMIT); fi; \
		if [ ! -f $$wasm ]; then \
			echo "ERROR: missing WASM for $$contract ($$wasm)" >&2; \
			status=1; \
			continue; \
		fi; \
		size=$$(wc -c < $$wasm); \
		if [ $$size -gt $$budget ]; then \
			echo "FAIL  $$contract: $$size bytes exceeds budget of $$budget bytes" >&2; \
			status=1; \
		else \
			echo "OK    $$contract: $$size / $$budget bytes"; \
		fi; \
	done; \
	if [ $$status -ne 0 ]; then \
		echo "WASM size check failed: one or more contracts exceed their budget." >&2; \
		exit 1; \
	fi; \
	echo "All contracts within their WASM size budgets."

lint-clippy:
	cargo clippy --all-targets --all-features -- -D warnings

lint-fmt:
	cargo fmt --all -- --check

lint-audit:
	cargo audit

# Emit a Markdown table of contract WASM sizes for posting to a PR comment.
# Columns: contract | current size | previous size | delta.
# Contracts over 75% of the budget are highlighted yellow; over budget red.
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
