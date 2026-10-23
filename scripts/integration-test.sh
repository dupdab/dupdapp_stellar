#!/usr/bin/env bash
#
# Integration test runner for the full payment lifecycle (issue #1311).
#
# Runs against the local testnet brought up via docker-compose:
#   docker compose up -d
#
# Lifecycle steps:
#   1. deploy all contracts
#   2. register merchant
#   3. create escrow deposit
#   4. release escrow
#   5. record settlement
#
# Each step prints PASS/FAIL together with the transaction hash.

set -uo pipefail

# ---------------------------------------------------------------------------
# Configuration (overridable via environment)
# ---------------------------------------------------------------------------
NETWORK="${NETWORK:-local}"
RPC_URL="${RPC_URL:-http://localhost:8000/soroban/rpc}"
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Standalone Network ; February 2017}"
HORIZON_URL="${HORIZON_URL:-http://localhost:8000}"

# Identity used to sign lifecycle transactions.
SOURCE_ACCOUNT="${SOURCE_ACCOUNT:-integration-runner}"

# Contract WASM artifacts produced by `cargo build --target wasm32-unknown-unknown --release`.
WASM_DIR="${WASM_DIR:-dabdub_contracts/target/wasm32-unknown-unknown/release}"

# Contracts deployed as part of the lifecycle.
CONTRACTS=(
  "escrow"
  "merchant_registry"
  "settlement"
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
PASS_COUNT=0
FAIL_COUNT=0

log() { printf '%s\n' "$*"; }

# run_step <name> <command...>
# Executes a step, captures stdout, and reports PASS/FAIL with the tx hash.
run_step() {
  local name="$1"
  shift
  local output
  local status

  log ""
  log "==> ${name}"
  output="$("$@" 2>&1)"
  status=$?

  # Extract a transaction hash if the command emitted one.
  local tx_hash
  tx_hash="$(printf '%s\n' "$output" | grep -Eo '[0-9a-fA-F]{64}' | head -n1 || true)"

  if [ "$status" -eq 0 ]; then
    PASS_COUNT=$((PASS_COUNT + 1))
    log "PASS: ${name}"
    [ -n "$tx_hash" ] && log "  tx: ${tx_hash}"
    return 0
  fi

  FAIL_COUNT=$((FAIL_COUNT + 1))
  log "FAIL: ${name} (exit ${status})"
  [ -n "$tx_hash" ] && log "  tx: ${tx_hash}"
  printf '%s\n' "$output" | sed 's/^/  /'
  return 1
}

# ---------------------------------------------------------------------------
# Preflight: ensure the local testnet is reachable.
# ---------------------------------------------------------------------------
log "Checking local testnet at ${RPC_URL} ..."
if ! curl -sf "${HORIZON_URL}/health" >/dev/null 2>&1; then
  log "ERROR: local testnet not reachable at ${HORIZON_URL}."
  log "       Start it with: docker compose up -d"
  exit 1
fi

# Ensure the signing identity exists (idempotent).
if ! stellar keys address "${SOURCE_ACCOUNT}" >/dev/null 2>&1; then
  stellar keys generate "${SOURCE_ACCOUNT}" --network "${NETWORK}" --fund >/dev/null 2>&1 || true
fi

# ---------------------------------------------------------------------------
# Step 1: deploy all contracts
# ---------------------------------------------------------------------------
DEPLOYED=()
for contract in "${CONTRACTS[@]}"; do
  wasm="${WASM_DIR}/${contract}.wasm"
  if [ ! -f "${wasm}" ]; then
    log "ERROR: missing wasm artifact ${wasm}"
    log "       Build with: cargo build --target wasm32-unknown-unknown --release"
    exit 1
  fi

  deploy_cmd() {
    stellar contract deploy \
      --wasm "${wasm}" \
      --source "${SOURCE_ACCOUNT}" \
      --network "${NETWORK}" \
      --rpc-url "${RPC_URL}" \
      --network-passphrase "${NETWORK_PASSPHRASE}"
  }

  if run_step "deploy ${contract}" deploy_cmd; then
    DEPLOYED+=("${contract}")
  fi
done

# ---------------------------------------------------------------------------
# Step 2: register merchant
# ---------------------------------------------------------------------------
MERCHANT_ID="${MERCHANT_ID:-merchant-integration-001}"

register_merchant() {
  stellar contract invoke \
    --id "${MERCHANT_REGISTRY_ID:?MERCHANT_REGISTRY_ID not set}" \
    --source "${SOURCE_ACCOUNT}" \
    --network "${NETWORK}" \
    --rpc-url "${RPC_URL}" \
    --network-passphrase "${NETWORK_PASSPHRASE}" \
    -- register_merchant \
    --merchant_id "${MERCHANT_ID}"
}

run_step "register merchant ${MERCHANT_ID}" register_merchant

# ---------------------------------------------------------------------------
# Step 3: create escrow deposit
# ---------------------------------------------------------------------------
ESCROW_AMOUNT="${ESCROW_AMOUNT:-1000}"

create_escrow() {
  stellar contract invoke \
    --id "${ESCROW_ID:?ESCROW_ID not set}" \
    --source "${SOURCE_ACCOUNT}" \
    --network "${NETWORK}" \
    --rpc-url "${RPC_URL}" \
    --network-passphrase "${NETWORK_PASSPHRASE}" \
    -- create_escrow \
    --merchant_id "${MERCHANT_ID}" \
    --amount "${ESCROW_AMOUNT}"
}

run_step "create escrow deposit (${ESCROW_AMOUNT})" create_escrow

# ---------------------------------------------------------------------------
# Step 4: release escrow
# ---------------------------------------------------------------------------
release_escrow() {
  stellar contract invoke \
    --id "${ESCROW_ID:?ESCROW_ID not set}" \
    --source "${SOURCE_ACCOUNT}" \
    --network "${NETWORK}" \
    --rpc-url "${RPC_URL}" \
    --network-passphrase "${NETWORK_PASSPHRASE}" \
    -- release \
    --merchant_id "${MERCHANT_ID}"
}

run_step "release escrow" release_escrow

# ---------------------------------------------------------------------------
# Step 5: record settlement
# ---------------------------------------------------------------------------
record_settlement() {
  stellar contract invoke \
    --id "${SETTLEMENT_ID:?SETTLEMENT_ID not set}" \
    --source "${SOURCE_ACCOUNT}" \
    --network "${NETWORK}" \
    --rpc-url "${RPC_URL}" \
    --network-passphrase "${NETWORK_PASSPHRASE}" \
    -- record_settlement \
    --merchant_id "${MERCHANT_ID}" \
    --amount "${ESCROW_AMOUNT}"
}

run_step "record settlement" record_settlement

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
log ""
log "----------------------------------------"
log "Integration lifecycle: ${PASS_COUNT} passed, ${FAIL_COUNT} failed"
log "----------------------------------------"

if [ "${FAIL_COUNT}" -ne 0 ]; then
  exit 1
fi
exit 0
