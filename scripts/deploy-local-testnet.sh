#!/usr/bin/env bash
# Deploy dabdub_contracts to the local Soroban testnet started via `make local-testnet-start`.
set -euo pipefail

RPC_URL="${SOROBAN_RPC_URL:-http://localhost:8000/soroban/rpc}"
NETWORK_PASSPHRASE="${SOROBAN_NETWORK_PASSPHRASE:-Standalone Network ; February 2017}"
SOURCE_ACCOUNT="${SOROBAN_SOURCE_ACCOUNT:-dabdub-deployer}"

CONTRACTS=(admin_auth admin_timelock batch_payments escrow fee_calculator fee_distributor liquidity_router registry)

if ! command -v soroban >/dev/null 2>&1; then
  echo "error: 'soroban' CLI not found in PATH" >&2
  exit 1
fi

echo "Building contracts..."
cargo build --workspace --target wasm32-unknown-unknown --release

if ! soroban keys address "$SOURCE_ACCOUNT" >/dev/null 2>&1; then
  echo "Generating deployer identity '$SOURCE_ACCOUNT'..."
  soroban keys generate "$SOURCE_ACCOUNT" --network local --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE"
fi

for contract in "${CONTRACTS[@]}"; do
  wasm="target/wasm32-unknown-unknown/release/${contract}.wasm"
  if [[ ! -f "$wasm" ]]; then
    echo "warning: $wasm not found, skipping $contract" >&2
    continue
  fi
  echo "Deploying $contract..."
  soroban contract deploy \
    --wasm "$wasm" \
    --source "$SOURCE_ACCOUNT" \
    --network local \
    --rpc-url "$RPC_URL" \
    --network-passphrase "$NETWORK_PASSPHRASE"
done

echo "Done. Contracts deployed to $RPC_URL"
