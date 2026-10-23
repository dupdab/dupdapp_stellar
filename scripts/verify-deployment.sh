#!/usr/bin/env bash
#
# Verify Soroban testnet deployments for the dabdub contracts.
#
# For each deployed contract this script invokes get_admin, get_version and a
# basic read query, printing OK/FAIL per contract. It exits non-zero if any
# contract fails verification.
#
# Environment:
#   STELLAR_NETWORK   network to target (default: testnet)
#   SOROBAN_RPC_URL   optional RPC URL override
#   SOURCE_ACCOUNT    identity used for read-only invocations (default: deployer)
#   CONTRACT_<NAME>   optional explicit contract id override per contract
#
set -uo pipefail

NETWORK="${STELLAR_NETWORK:-testnet}"
SOURCE_ACCOUNT="${SOURCE_ACCOUNT:-deployer}"

# Contract name -> basic read query function to exercise.
# Format: "name:read_function"
CONTRACTS=(
  "admin_auth:get_admin"
  "admin_timelock:get_admin"
  "payment_escrow:get_admin"
)

RPC_ARGS=()
if [ -n "${SOROBAN_RPC_URL:-}" ]; then
  RPC_ARGS=(--rpc-url "${SOROBAN_RPC_URL}")
fi

if ! command -v stellar >/dev/null 2>&1; then
  echo "FAIL  stellar CLI not found in PATH" >&2
  exit 1
fi

# Resolve a contract id from an env override or the local deployment file.
resolve_contract_id() {
  local name="$1"
  local env_var
  env_var="CONTRACT_$(echo "$name" | tr '[:lower:]' '[:upper:]')"

  if [ -n "${!env_var:-}" ]; then
    echo "${!env_var}"
    return 0
  fi

  local deploy_file="${CONTRACTS_DIR:-dabdub_contracts}/.deployments/${NETWORK}.json"
  if [ -f "$deploy_file" ]; then
    # Extract the contract id for this name from the deployment manifest.
    grep -o "\"${name}\"[[:space:]]*:[[:space:]]*\"[^\"]*\"" "$deploy_file" \
      | head -n1 \
      | sed -E 's/.*:[[:space:]]*"([^"]*)"/\1/'
    return 0
  fi

  return 1
}

# Invoke a read-only function and return its output; non-zero on failure.
invoke_read() {
  local contract_id="$1"
  local function_name="$2"
  stellar contract invoke \
    --id "$contract_id" \
    --network "$NETWORK" \
    --source "$SOURCE_ACCOUNT" \
    "${RPC_ARGS[@]}" \
    -- "$function_name" 2>&1
}

failed=0

for entry in "${CONTRACTS[@]}"; do
  name="${entry%%:*}"
  read_fn="${entry##*:}"

  contract_id="$(resolve_contract_id "$name")"
  if [ -z "$contract_id" ]; then
    echo "FAIL  ${name}  (no contract id found for network '${NETWORK}')"
    failed=1
    continue
  fi

  contract_failed=0

  # get_admin
  if ! admin_out="$(invoke_read "$contract_id" "get_admin")"; then
    echo "  FAIL  ${name}.get_admin"
    contract_failed=1
  fi

  # get_version
  if ! version_out="$(invoke_read "$contract_id" "get_version")"; then
    echo "  FAIL  ${name}.get_version"
    contract_failed=1
  fi

  # basic read query
  if ! read_out="$(invoke_read "$contract_id" "$read_fn")"; then
    echo "  FAIL  ${name}.${read_fn}"
    contract_failed=1
  fi

  if [ "$contract_failed" -eq 0 ]; then
    echo "OK    ${name}  (${contract_id})"
  else
    echo "FAIL  ${name}  (${contract_id})"
    failed=1
  fi
done

if [ "$failed" -ne 0 ]; then
  echo "Deployment verification failed for one or more contracts." >&2
  exit 1
fi

echo "All contracts verified successfully on '${NETWORK}'."
exit 0
