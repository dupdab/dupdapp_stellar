CONTRACTS_DIR := dupdub_contracts
WASM_DIR      := $(CONTRACTS_DIR)/target/wasm32v1-none/release
WASM_SIZE_LIMIT := 65536  # 64 KB

.PHONY: build build-optimised check-wasm-size test

build:
	cd $(CONTRACTS_DIR) && cargo build --target wasm32v1-none --release

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
