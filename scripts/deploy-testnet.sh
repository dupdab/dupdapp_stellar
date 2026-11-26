#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
CONTRACTS_DIR="$ROOT_DIR/dabdub_contracts"
WASM_DIR="$CONTRACTS_DIR/target/wasm32v1-none/release"
DEPLOYMENTS_FILE="$ROOT_DIR/deployments/testnet.json"
RPC_URL="${STELLAR_TESTNET_RPC_URL:-https://soroban-testnet.stellar.org}"
NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
SOURCE_ACCOUNT="${SOROBAN_SOURCE_ACCOUNT:-deployer}"

if ! command -v stellar >/dev/null 2>&1; then
  echo "error: stellar CLI is required (install stellar-cli)" >&2
  exit 1
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo is required to build the contracts" >&2
  exit 1
fi
if ! command -v node >/dev/null 2>&1; then
  echo "error: node is required to write deployments/testnet.json" >&2
  exit 1
fi

required_vars=(
  DEPLOY_ADMIN
  RBAC_SUPER_ADMIN
  XLM_TOKEN
  USDC_TOKEN
  TREASURY
  LP_ADDRESS
  LP_SHARE_BPS
  FEE_DISTRIBUTOR_ALLOWED_CALLER
  FEE_TIERS
  MIN_FEE_BPS
  MULTISIG_ADMIN_1
  MULTISIG_ADMIN_2
  MULTISIG_ADMIN_3
  ESCROW_DEFAULT_TTL_LEDGERS
  EMERGENCY_SIGNERS
  EMERGENCY_TREASURY
  EMERGENCY_COOLDOWN_LEDGERS
  CONFIRMATION_COUNT
  BATCH_PAYMENTS_MIN_AMOUNT
  BATCH_PAYMENTS_MAX_AMOUNT
)

missing=0
for variable in "${required_vars[@]}"; do
  if [[ -z "${!variable:-}" ]]; then
    echo "error: $variable must be set before deploying to testnet" >&2
    missing=1
  fi
done
if [[ "$missing" -ne 0 ]]; then
  exit 1
fi

deployer_address="$(stellar keys address "$SOURCE_ACCOUNT")"
if [[ "$deployer_address" != "$DEPLOY_ADMIN" ]]; then
  echo "error: $SOURCE_ACCOUNT must be the DEPLOY_ADMIN signer (got $deployer_address)" >&2
  exit 1
fi

cd "$CONTRACTS_DIR"
echo "Building all Soroban contracts for testnet..."
cargo build --workspace --target wasm32v1-none --release

stellar network add testnet \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE"

mkdir -p "$(dirname "$DEPLOYMENTS_FILE")"
node - "$DEPLOYMENTS_FILE" "$RPC_URL" <<'NODE'
const fs = require("node:fs");
const [file, rpcUrl] = process.argv.slice(2);
fs.writeFileSync(file, JSON.stringify({
  network: "testnet",
  rpc_url: rpcUrl,
  deployed_at: new Date().toISOString(),
  contracts: {},
}, null, 2) + "\n");
NODE

save_contract_id() {
  local name="$1"
  local contract_id="$2"
  CONTRACT_NAME="$name" CONTRACT_ID="$contract_id" node - "$DEPLOYMENTS_FILE" <<'NODE'
const fs = require("node:fs");
const [file] = process.argv.slice(2);
const deployment = JSON.parse(fs.readFileSync(file, "utf8"));
deployment.contracts[process.env.CONTRACT_NAME] = process.env.CONTRACT_ID;
fs.writeFileSync(file, JSON.stringify(deployment, null, 2) + "\n");
NODE
  printf -v "${name}_id" '%s' "$contract_id"
  export "${name}_id"
  echo "Deployed $name: $contract_id"
}

deploy_contract() {
  local name="$1"
  local wasm_name="$2"
  shift 2
  local wasm="$WASM_DIR/$wasm_name.wasm"
  local output
  if [[ ! -f "$wasm" ]]; then
    echo "error: expected WASM not found: $wasm" >&2
    exit 1
  fi

  if [[ "$#" -gt 0 ]]; then
    output="$(stellar contract deploy --wasm "$wasm" --source "$SOURCE_ACCOUNT" --network testnet -- "$@" 2>&1)"
  else
    output="$(stellar contract deploy --wasm "$wasm" --source "$SOURCE_ACCOUNT" --network testnet 2>&1)"
  fi
  local contract_id
  contract_id="$(printf '%s\n' "$output" | grep -Eo 'C[A-Z2-7]{55}' | tail -n 1 || true)"
  if [[ ! "$contract_id" =~ ^C[A-Z2-7]{55}$ ]]; then
    echo "error: could not parse contract ID for $name from Stellar CLI output:" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi
  save_contract_id "$name" "$contract_id"
}

invoke_contract() {
  local contract_id="$1"
  shift
  stellar contract invoke --id "$contract_id" --source "$SOURCE_ACCOUNT" --network testnet -- "$@"
}

register_contract_id() {
  local name="$1"
  local contract_id="$2"
  invoke_contract "$contract_registry_id" set_address --name "$name" --address "$contract_id"
}

deploy_contract contract_registry contract_registry
invoke_contract "$contract_registry_id" initialize --admin "$DEPLOY_ADMIN"

deploy_contract liquidity_router liquidity_router
invoke_contract "$liquidity_router_id" initialize --admin "$DEPLOY_ADMIN"

deploy_contract payment_request payment_request
invoke_contract "$payment_request_id" initialize --admin "$DEPLOY_ADMIN"

deploy_contract batch_payments batch_payments
invoke_contract "$batch_payments_id" init \
  --admin "$DEPLOY_ADMIN" \
  --min_amount "$BATCH_PAYMENTS_MIN_AMOUNT" \
  --max_amount "$BATCH_PAYMENTS_MAX_AMOUNT"

deploy_contract admin_auth admin_auth --super_admin "$DEPLOY_ADMIN"
deploy_contract admin_timelock admin_timelock --admin "$DEPLOY_ADMIN"
deploy_contract merchant_registry merchant_registry --admin "$DEPLOY_ADMIN"
deploy_contract reconciliation reconciliation --admin "$DEPLOY_ADMIN"
deploy_contract settlement_ledger settlement_ledger --admin "$DEPLOY_ADMIN"
deploy_contract slippage_protection slippage_protection --admin "$DEPLOY_ADMIN"
invoke_contract "$liquidity_router_id" set_slippage_protection \
  --admin "$DEPLOY_ADMIN" \
  --slippage_protection "$slippage_protection_id"
deploy_contract rbac_access rbac_access --super_admin "$RBAC_SUPER_ADMIN"
deploy_contract platform_stats platform_stats --admin "$DEPLOY_ADMIN"

deploy_contract stellar_confirmations stellar_confirmations \
  --admin "$DEPLOY_ADMIN" \
  --confirmation_count "$CONFIRMATION_COUNT"
deploy_contract multisig_admin multisig_admin \
  --admin1 "$MULTISIG_ADMIN_1" \
  --admin2 "$MULTISIG_ADMIN_2" \
  --admin3 "$MULTISIG_ADMIN_3"
deploy_contract fee_calculator fee_calculator \
  --admin "$DEPLOY_ADMIN" \
  --tiers "$FEE_TIERS" \
  --min_fee_bps "$MIN_FEE_BPS"
invoke_contract "$fee_calculator_id" set_settlement_caller \
  --caller "$DEPLOY_ADMIN" \
  --settlement_caller "$settlement_ledger_id"
deploy_contract fee_distributor fee_distributor \
  --admin "$DEPLOY_ADMIN" \
  --treasury "$TREASURY" \
  --lp_address "$LP_ADDRESS" \
  --lp_share_bps "$LP_SHARE_BPS" \
  --usdc_token "$USDC_TOKEN" \
  --allowed_caller "$FEE_DISTRIBUTOR_ALLOWED_CALLER"

deploy_contract payment_escrow payment_escrow \
  --admin "$DEPLOY_ADMIN" \
  --xlm_token "$XLM_TOKEN" \
  --usdc_token "$USDC_TOKEN" \
  --default_ttl_ledgers "$ESCROW_DEFAULT_TTL_LEDGERS" \
  --registry "${REGISTRY_ADDRESS:-$merchant_registry_id}" \
  --emergency_signers "$EMERGENCY_SIGNERS" \
  --emergency_treasury "$EMERGENCY_TREASURY" \
  --emergency_cooldown_ledgers "$EMERGENCY_COOLDOWN_LEDGERS"
invoke_contract "$stellar_confirmations_id" set_payment_escrow_contract \
  --caller "$DEPLOY_ADMIN" \
  --payment_escrow "$payment_escrow_id"

for contract in \
  admin_auth admin_timelock batch_payments fee_calculator fee_distributor \
  liquidity_router merchant_registry multisig_admin payment_escrow \
  payment_request platform_stats rbac_access reconciliation settlement_ledger \
  slippage_protection stellar_confirmations; do
  id_variable="${contract}_id"
  register_contract_id "$contract" "${!id_variable}"
done

echo "All 17 contracts deployed to $RPC_URL"
echo "Contract IDs recorded in $DEPLOYMENTS_FILE"
