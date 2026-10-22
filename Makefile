CONTRACTS_DIR := dabdub_contracts
WASM_DIR      := $(CONTRACTS_DIR)/target/wasm32v1-none/release
WASM_SIZE_LIMIT := 65536  # 64 KB

# Contracts that have per-contract test/build targets.
CONTRACTS := escrow registry

.PHONY: build build-optimised check-wasm-size test clean help \
        $(addprefix build-,$(CONTRACTS)) $(addprefix test-,$(CONTRACTS))

build:
	cd $(CONTRACTS_DIR) && cargo build --target wasm32v1-none --release

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

check-wasm-size: build-optimised
	@echo "Checking WASM sizes (limit: $(WASM_SIZE_LIMIT) bytes)..."
	@failed=0; \
	for wasm in $(WASM_DIR)/*.wasm; do \
		size=$$(wc -c < $$wasm); \
		if [ $$size -gt $(WASM_SIZE_LIMIT) ]; then \
			echo "  FAIL  $$wasm  $$size bytes  (limit $(WASM_SIZE_LIMIT))"; \
			failed=1; \
		else \
			echo "  OK    $$wasm  $$size bytes"; \
		fi; \
	done; \
	if [ $$failed -ne 0 ]; then \
		echo "One or more contracts exceed the $(WASM_SIZE_LIMIT)-byte limit."; \
		exit 1; \
	fi

test:
	cd $(CONTRACTS_DIR) && cargo test

# Per-contract test: `make test-<contract>` tests a single contract.
$(addprefix test-,$(CONTRACTS)): test-%:
	cd $(CONTRACTS_DIR) && cargo test -p $*

clean:
	cd $(CONTRACTS_DIR) && cargo clean

help:
	@echo "Available targets:"
	@echo "  build                 Build all contracts (wasm32v1-none, release)"
	@echo "  build-optimised       Build all contracts and run wasm-opt -Oz"
	@echo "  check-wasm-size       Verify all WASMs are within the size limit"
	@echo "  test                  Run the full test suite"
	@echo "  clean                 Remove all build artifacts"
	@echo "  help                  Show this help message"
	@for c in $(CONTRACTS); do \
		echo "  build-$$c             Build the $$c contract"; \
		echo "  test-$$c              Test the $$c contract"; \
	done
