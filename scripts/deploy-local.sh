#!/usr/bin/env bash
# Deploy dabdub_contracts to a local Soroban testnet (stellar/quickstart).
#
# Prerequisites:
#   - Docker + docker compose
#   - stellar CLI (https://developers.stellar.org/docs/tools/developer-tools/cli/install-cli)
#   - Local testnet running: `make local-testnet-start`
#
# Usage:
#   ./scripts/deploy-local.sh
#
# Environment overrides:
#   RPC_URL        (default: http://localhost:8000/soroban/rpc)
#   NETWORK_PASSPHRASE (default: quickstart local network passphrase)
#   SOURCE_ACCOUNT (default: quickstart funded account)
#   CONTRACT       (default: payment_escrow)

set -euo pipefail

RPC_URL="${RPC_URL:-http://localhost:8000/soroban/rpc}"
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Standalone Network ; February 2017}"
SOURCE_ACCOUNT="${SOURCE_ACCOUNT:-admin}"
CONTRACT="${CONTRACT:-payment_escrow}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONTRACTS_DIR="${REPO_ROOT}/dabdub_contracts"
WASM_PATH="${CONTRACTS_DIR}/target/wasm32-unknown-unknown/release/${CONTRACT}.wasm"

if ! command -v stellar >/dev/null 2>&1; then
  echo "error: stellar CLI not found in PATH" >&2
  echo "install: https://developers.stellar.org/docs/tools/developer-tools/cli/install-cli" >&2
  exit 1
fi

if ! curl -sf -X POST "${RPC_URL}" \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' >/dev/null 2>&1; then
  echo "error: no Soroban RPC responding at ${RPC_URL}" >&2
  echo "hint: run 'make local-testnet-start' first" >&2
  exit 1
fi

echo "==> Building ${CONTRACT} wasm"
(cd "${CONTRACTS_DIR}" && cargo build --release --target wasm32-unknown-unknown -p "${CONTRACT}")

if [ ! -f "${WASM_PATH}" ]; then
  echo "error: wasm not found at ${WASM_PATH}" >&2
  exit 1
fi

echo "==> Deploying ${CONTRACT} to ${RPC_URL}"
CONTRACT_ID="$(stellar contract deploy \
  --wasm "${WASM_PATH}" \
  --source "${SOURCE_ACCOUNT}" \
  --rpc-url "${RPC_URL}" \
  --network-passphrase "${NETWORK_PASSPHRASE}")"

echo "==> Deployed ${CONTRACT}: ${CONTRACT_ID}"
echo "export CONTRACT_ID=${CONTRACT_ID}"
