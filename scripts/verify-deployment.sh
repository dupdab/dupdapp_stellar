#!/usr/bin/env bash
# Verify a Soroban testnet deployment by calling basic read queries on each
# deployed contract. Prints OK / FAIL per contract and exits non-zero if any
# contract fails verification.
#
# Usage:
#   scripts/verify-deployment.sh
#
# Environment:
#   SOROBAN_NETWORK   Network name passed to `soroban contract invoke` (default: testnet)
#   SOROBAN_RPC_URL   Optional RPC URL override
#   ADMIN_AUTH_ID     Contract ID for admin_auth
#   PAYMENT_ESCROW_ID Contract ID for payment_escrow
#   MERCHANT_REGISTRY_ID Contract ID for merchant_registry
#   ADMIN_ADDRESS     Address used for get_admin read query
#
# Contract IDs may also be read from a deployment manifest at
# deployments/<network>.json (keys: admin_auth, payment_escrow, merchant_registry).

set -uo pipefail

NETWORK="${SOROBAN_NETWORK:-testnet}"
MANIFEST="deployments/${NETWORK}.json"

# Load contract IDs from manifest if present and not already set.
if [[ -f "$MANIFEST" ]]; then
  if command -v jq >/dev/null 2>&1; then
    ADMIN_AUTH_ID="${ADMIN_AUTH_ID:-$(jq -r '.admin_auth // empty' "$MANIFEST" 2>/dev/null)}"
    PAYMENT_ESCROW_ID="${PAYMENT_ESCROW_ID:-$(jq -r '.payment_escrow // empty' "$MANIFEST" 2>/dev/null)}"
    MERCHANT_REGISTRY_ID="${MERCHANT_REGISTRY_ID:-$(jq -r '.merchant_registry // empty' "$MANIFEST" 2>/dev/null)}"
  fi
fi

ADMIN_AUTH_ID="${ADMIN_AUTH_ID:-}"
PAYMENT_ESCROW_ID="${PAYMENT_ESCROW_ID:-}"
MERCHANT_REGISTRY_ID="${MERCHANT_REGISTRY_ID:-}"
ADMIN_ADDRESS="${ADMIN_ADDRESS:-}"

FAILURES=0

invoke() {
  local contract_id="$1"
  shift
  local args=(contract invoke --id "$contract_id" --network "$NETWORK")
  if [[ -n "${SOROBAN_RPC_URL:-}" ]]; then
    args+=(--rpc-url "$SOROBAN_RPC_URL")
  fi
  soroban "${args[@]}" "$@" >/dev/null 2>&1
}

# verify_contract <name> <contract_id> <query...>
# Runs each query; prints OK if all succeed, FAIL otherwise.
verify_contract() {
  local name="$1"
  local contract_id="$2"
  shift 2

  if [[ -z "$contract_id" ]]; then
    echo "FAIL $name (no contract id configured)"
    FAILURES=$((FAILURES + 1))
    return
  fi

  local failed=0
  local query
  for query in "$@"; do
    # shellcheck disable=SC2086
    if ! invoke "$contract_id" $query; then
      echo "  query failed: $query"
      failed=1
    fi
  done

  if [[ $failed -eq 0 ]]; then
    echo "OK   $name ($contract_id)"
  else
    echo "FAIL $name ($contract_id)"
    FAILURES=$((FAILURES + 1))
  fi
}

# admin_auth: get_admin + get_version
ADMIN_QUERIES=("get_version")
if [[ -n "$ADMIN_ADDRESS" ]]; then
  ADMIN_QUERIES+=("get_admin --admin $ADMIN_ADDRESS")
fi
verify_contract "admin_auth" "$ADMIN_AUTH_ID" "${ADMIN_QUERIES[@]}"

# payment_escrow: get_version + basic read query
verify_contract "payment_escrow" "$PAYMENT_ESCROW_ID" "get_version"

# merchant_registry: get_version + basic read query
verify_contract "merchant_registry" "$MERCHANT_REGISTRY_ID" "get_version"

echo ""
if [[ $FAILURES -ne 0 ]]; then
  echo "Deployment verification failed: $FAILURES contract(s) failed."
  exit 1
fi

echo "Deployment verification passed."
exit 0
