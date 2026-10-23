#!/usr/bin/env bash
# Verifies that the contract WASMs built in this repository are within the
# deployment constraints enforced by CI (the 64KB per-contract WASM budget).
#
# Usage: scripts/verify-deployment.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

WASM_DIR="${WASM_DIR:-$REPO_ROOT/dupdub_contracts/target/wasm32v1-none/release}"
WASM_SIZE_LIMIT="${WASM_SIZE_LIMIT:-65536}"
WASM_SIZE_WARN="${WASM_SIZE_WARN:-52428}"

export WASM_DIR WASM_SIZE_LIMIT WASM_SIZE_WARN

echo "==> Verifying contract WASM size budget in $WASM_DIR"
bash "$SCRIPT_DIR/check-wasm-size.sh"
echo "==> Deployment verification passed."
