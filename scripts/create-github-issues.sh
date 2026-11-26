#!/usr/bin/env bash
# Creates 125 GitHub issues for dupdapp_stellar
set -e
REPO="dupdab/dupdapp-stellar"
ci() {
  local title="$1" body="$2" labels="$3"
  gh issue create --repo "$REPO" --title "$title" --body "$body" --label "$labels" 2>/dev/null || \
  gh issue create --repo "$REPO" --title "$title" --body "$body" 2>/dev/null
  sleep 0.35
}
echo "Creating 125 issues for dupdapp_stellar..."

# ── P0 CRITICAL (15) ──────────────────────────────────────────────────────────

ci "[P0] payment_escrow: \`expire()\` and \`transfer_from_contract\` missing \`asset_type\` parameter — compile error" \
"## Problem
In \`contracts/payment_escrow/src/lib.rs\`, the \`expire()\` function calls \`Self::transfer_from_contract(&env, &payment.customer, remaining)\` without the required \`asset_type\` parameter that \`transfer_from_contract\` now requires. This is a compile-time error that blocks all builds.

## Acceptance Criteria
- [ ] Fix \`expire()\` to pass \`&payment.asset_type\` as the fourth argument
- [ ] Verify \`make build\` compiles without errors
- [ ] Add unit test for XLM expiry path
- [ ] Add unit test for USDC expiry path" \
"bug,P0,payment-escrow"

ci "[P0] payment_escrow: emergency_drain only drains USDC — XLM funds stranded in contract" \
"## Problem
\`emergency_drain\` in \`payment_escrow\` only fetches and transfers USDC balance. If payments were deposited in XLM (\`AssetType::Xlm\`), those XLM funds remain stranded in the contract after an emergency drain.

## Acceptance Criteria
- [ ] Drain both USDC and XLM balances in a single emergency_drain call
- [ ] Return total drained amount (sum of both)
- [ ] Unit tests: drain with only USDC, only XLM, and both
- [ ] Update EMERGENCY_RUNBOOK.md to reflect dual-asset drain" \
"bug,P0,payment-escrow"

ci "[P0] batch_payments: payment IDs derived from \`u64.to_be_bytes()\` — collision risk at same ledger" \
"## Problem
\`create_batch\` derives payment IDs as \`sha256(ledger_sequence * 1000 + index)\`. Two batches submitted in the same ledger (sequence) with the same index will produce identical payment IDs. There is a comment in the code acknowledging this is a placeholder.

## Acceptance Criteria
- [ ] Replace ID derivation with \`sha256(merchant || ledger_sequence || timestamp || index)\` for uniqueness
- [ ] Add fuzz test asserting no ID collision across 1000 batch submissions in the same ledger
- [ ] Document the ID derivation scheme in a code comment
- [ ] Deterministic IDs must survive contract upgrade (migration test)" \
"bug,P0,batch-payments"

ci "[P0] rbac_access: roles stored in \`instance\` storage — roles are lost on contract upgrade" \
"## Problem
\`RbacAccessContract\` stores all role assignments in \`env.storage().instance()\`. Instance storage is wiped on contract upgrade. After upgrading the contract, all role assignments are lost — including the SuperAdmin role.

## Acceptance Criteria
- [ ] Migrate role storage to \`env.storage().persistent()\`
- [ ] Add a constructor migration test verifying roles survive \`upgrade()\`
- [ ] Document storage type rationale in code comment
- [ ] Verify with Soroban upgrade testnet scenario" \
"bug,P0,rbac-access"

ci "[P0] settlement_ledger: no amount validation against fee_calculator — net can exceed amount" \
"## Problem
\`record_settlement\` validates \`fee + net == amount\` but does not verify that the fee was computed by the \`fee_calculator\` contract. The caller (NestJS backend) could record an incorrect fee (e.g., fee=0), and the assertion would still pass.

## Acceptance Criteria
- [ ] Cross-contract call to \`fee_calculator.calculate_fee(merchant, amount)\` to verify the fee is correct before recording
- [ ] Add \`fee_calculator_contract\` address to settlement_ledger constructor
- [ ] Unit test: attempt to record settlement with wrong fee — should panic
- [ ] Document trust boundary: only the backend admin can call record_settlement" \
"bug,P0,settlement-ledger"

ci "[P0] multisig_admin: contract not integrated with payment_escrow admin actions" \
"## Problem
A \`multisig_admin\` contract exists but \`payment_escrow\` admin actions (\`release\`, \`resolve_dispute\`) only check a single admin address. There is no multi-signature requirement for high-value operations.

## Acceptance Criteria
- [ ] Define which escrow operations require multisig threshold (e.g., release > 10,000 USDC)
- [ ] payment_escrow calls multisig_admin to verify quorum before executing
- [ ] Unit test: single-signer release of large payment fails; 2-of-3 succeeds
- [ ] Document multisig policy in EMERGENCY_RUNBOOK.md" \
"bug,P0,multisig-admin"

ci "[P0] admin_timelock: not integrated with any contract — timelocked upgrades not enforced" \
"## Problem
\`admin_timelock\` contract exists but no other contract routes upgrade operations through it. WASM upgrades can happen immediately without timelock delay.

## Acceptance Criteria
- [ ] payment_escrow upgrade() must first queue action in admin_timelock
- [ ] Minimum timelock delay: 48 hours (configurable via env)
- [ ] cancel_timelock() must exist for admin to cancel queued upgrades
- [ ] Integration test: upgrade before timelock expires must panic" \
"bug,P0,admin-timelock"

ci "[P0] fee_calculator: merchant volume window uses ledger sequence, not timestamp — inconsistent periods" \
"## Problem
The 30-day volume window is calculated as \`LEDGERS_PER_30_DAYS = 172_800\` ledgers. Ledger close times can vary (normally ~5s but can be longer), making the actual window anywhere from 24–36 days. Fee tiers based on this may be incorrect.

## Acceptance Criteria
- [ ] Use \`env.ledger().timestamp()\` (Unix seconds) for the rolling window instead of ledger sequence
- [ ] Define \`WINDOW_SECONDS: u64 = 30 * 24 * 60 * 60\`
- [ ] Update \`MerchantVolume\` to store \`window_start_timestamp: u64\`
- [ ] Add test with manipulated ledger timestamps to verify correct expiry" \
"bug,P0,fee-calculator"

ci "[P0] payment_escrow: no re-entrancy guard on \`release\` and \`refund\` — token callback re-entry possible" \
"## Problem
\`release\` and \`refund\` call \`token::Client::transfer\` which invokes the recipient's contract. A malicious merchant contract could re-enter \`release\` during the transfer callback, double-claiming funds before the status is updated.

## Acceptance Criteria
- [ ] Update payment status to \`Released\`/\`Expired\` BEFORE calling token transfer (checks-effects-interactions)
- [ ] Add reentrancy test: mock recipient that calls release() again during transfer
- [ ] Document the pattern in a code comment
- [ ] Review all other fund-transfer paths for the same pattern" \
"security,P0,payment-escrow"

ci "[P0] All contracts: \`panic!\` used for errors — replace with structured error enums" \
"## Problem
All 14 contracts use \`panic!(\"message\")\` for error handling. This means error conditions are indistinguishable to the caller (NestJS backend) — all panics look the same. Soroban supports error enums for structured contract errors.

## Acceptance Criteria
- [ ] Define a \`ContractError\` enum per contract using \`#[contracterror]\`
- [ ] Replace all \`panic!()\` calls with \`Err(ContractError::Variant)\`
- [ ] Backend can pattern-match on specific error types
- [ ] All existing tests updated to check error variants" \
"enhancement,P0,architecture"

ci "[P0] payment_escrow: WASM binary not enforced under 64KB limit in CI" \
"## Problem
\`make check-wasm-size\` exists but its enforcement in CI is unclear. If the payment_escrow WASM exceeds 64KB after future changes, the contract cannot be deployed to Stellar mainnet.

## Acceptance Criteria
- [ ] CI step runs \`make check-wasm-size\` and fails build if limit exceeded
- [ ] Add size budget comment in Cargo.toml
- [ ] Measure current WASM size and document it
- [ ] Add \`wasm-opt -Oz\` as part of the release build check" \
"bug,P0,payment-escrow"

ci "[P0] reconciliation: contract has no tests — correctness unverified" \
"## Problem
The \`reconciliation\` contract has a \`test_snapshots/\` directory but no visible test coverage for its reconciliation logic. Settlement reconciliation is a critical financial operation.

## Acceptance Criteria
- [ ] Full unit test suite for reconciliation contract
- [ ] Test: successful reconciliation
- [ ] Test: reconciliation with mismatched amounts fails
- [ ] Test: duplicate reconciliation call fails
- [ ] Coverage >= 90% as measured by tarpaulin" \
"testing,P0,reconciliation"

ci "[P0] fee_distributor: contract has no tests and no documented interface" \
"## Problem
\`fee_distributor\` has no test directory and no documentation on how fees are distributed to recipients. This contract handles real money.

## Acceptance Criteria
- [ ] Write complete test suite
- [ ] Document: who are fee recipients, what percentage each receives
- [ ] Test: correct split between platform, liquidity provider, operational wallet
- [ ] Test: cannot distribute more than collected
- [ ] Integration test with fee_calculator output" \
"testing,P0,fee-distributor"

ci "[P0] stellar_confirmations: contract has no tests and unclear purpose" \
"## Problem
\`stellar_confirmations\` has no test directory and no README explaining what it confirms or how it interacts with other contracts.

## Acceptance Criteria
- [ ] Document: what events are confirmed, confirmation thresholds, who calls this contract
- [ ] Full unit test suite
- [ ] Integration test with payment_escrow deposit event
- [ ] Clarify if this is still needed or superseded by Soroban event indexing in the backend" \
"testing,P0,stellar-confirmations"

ci "[P0] payment_request: contract has no tests" \
"## Problem
\`payment_request\` has no \`test_snapshots/\` directory and no test coverage. Payment requests are the entry point to the entire payment flow.

## Acceptance Criteria
- [ ] Full unit test suite for payment_request contract
- [ ] Test: create payment request
- [ ] Test: duplicate payment request ID fails
- [ ] Test: expired payment request cannot be fulfilled
- [ ] Test: cancelled payment request cannot be fulfilled" \
"testing,P0,payment-request"

# ── P1 HIGH (30) ──────────────────────────────────────────────────────────────

ci "[P1] payment_escrow: add \`release_to_customer\` path for admin-approved refunds" \
"## Problem
\`release()\` always sends funds to the merchant. There is no admin path to release escrow funds back to the customer without going through the dispute flow.

## Acceptance Criteria
- [ ] Add \`refund_admin(env, caller, payment_id)\` — admin-only direct refund
- [ ] Requires multisig approval for amounts > configurable threshold
- [ ] Emits \`('ESCROW', 'admin_refund')\` event
- [ ] Unit tests for happy path and permission checks" \
"enhancement,P1,payment-escrow"

ci "[P1] merchant_registry: add merchant metadata fields (website, contact, country)" \
"## Problem
\`MerchantRecord\` only stores \`name\` and \`status\`. Off-chain merchant data (website, country, contact) needs to match the on-chain record for compliance purposes.

## Acceptance Criteria
- [ ] Add optional \`metadata: Map<String, String>\` to \`MerchantRecord\`
- [ ] \`update_metadata(caller, merchant, key, value)\` — admin or merchant self-service
- [ ] Max metadata size enforced to stay within storage limits
- [ ] Unit tests for CRUD on metadata" \
"enhancement,P1,merchant-registry"

ci "[P1] fee_calculator: add minimum fee floor to prevent zero-fee gaming" \
"## Problem
If a merchant's volume places them in a 0 bps tier, they pay no fees. The contract should enforce a minimum fee floor (e.g., 10 bps).

## Acceptance Criteria
- [ ] Add \`min_fee_bps: u32\` to constructor (default: 10 bps)
- [ ] \`calculate_fee\` applies \`max(calculated_fee, min_fee_floor)\`
- [ ] Admin can update min fee floor
- [ ] Unit test: zero-bps tier still charges minimum floor" \
"enhancement,P1,fee-calculator"

ci "[P1] Add cross-contract integration tests between payment_escrow and merchant_registry" \
"## Problem
The escrow contract checks merchant approval via registry. This cross-contract interaction has no integration test. A registry upgrade could silently break escrow.

## Acceptance Criteria
- [ ] Integration test: deposit with unregistered merchant fails
- [ ] Integration test: deposit with suspended merchant fails
- [ ] Integration test: deposit with active+KYC merchant succeeds
- [ ] Test uses real contract clients (not mocks)" \
"testing,P1,payment-escrow"

ci "[P1] Add cross-contract integration tests between settlement_ledger and fee_calculator" \
"## Problem
Settlement recording uses the fee computed by fee_calculator but these are currently separate calls. No test verifies the end-to-end fee→settlement chain.

## Acceptance Criteria
- [ ] Integration test: calculate_fee → record_settlement with correct amounts
- [ ] Integration test: record_settlement with incorrect fee is rejected
- [ ] Test multi-tier fee calculation flowing into settlement record
- [ ] Document expected fee accuracy in README" \
"testing,P1,settlement-ledger"

ci "[P1] payment_escrow: add partial release tests for all asset types" \
"## Problem
\`release_partial\` is implemented but test coverage only covers the happy path. Edge cases (exact remaining, over-release, released after partial) lack tests.

## Acceptance Criteria
- [ ] Test: partial release of XLM payment
- [ ] Test: partial release of USDC payment
- [ ] Test: two partial releases summing to total → status becomes Released
- [ ] Test: partial release > remaining amount panics
- [ ] Test: partial release on already-Released payment panics" \
"testing,P1,payment-escrow"

ci "[P1] Add \`make deploy-testnet\` command and testnet deployment guide" \
"## Problem
The README says deploy workflow deploys to testnet on merge to main, but there is no guide for manually deploying to testnet for development testing.

## Acceptance Criteria
- [ ] \`make deploy-testnet\` deploys all contracts to Stellar testnet
- [ ] Script outputs deployed contract IDs
- [ ] Contract IDs saved to \`deployments/testnet.json\`
- [ ] README testnet deployment guide updated" \
"documentation,P1,devops"

ci "[P1] Implement contract address registry to avoid hardcoded addresses between contracts" \
"## Problem
Contract addresses (registry, fee_calculator, etc.) are passed as constructor arguments with no on-chain address book. If any contract is upgraded and redeployed, all dependent contracts need manual re-initialization.

## Acceptance Criteria
- [ ] New \`contract_registry\` contract: maps contract name → address
- [ ] payment_escrow looks up registry address from contract_registry at call time
- [ ] Admin can update addresses in registry
- [ ] Unit test: stale address detection" \
"enhancement,P1,architecture"

ci "[P1] Add contract version checking between dependent contracts" \
"## Problem
payment_escrow calls merchant_registry without verifying the registry contract version. A major registry upgrade that changes the \`is_approved\` ABI would silently fail.

## Acceptance Criteria
- [ ] payment_escrow checks \`registry_client.version() >= MIN_COMPATIBLE_VERSION\`
- [ ] MIN_COMPATIBLE_VERSION stored as a constant in escrow
- [ ] Panic with clear error if version incompatible
- [ ] Version bump process documented in README" \
"enhancement,P1,architecture"

ci "[P1] slippage_protection: add integration with liquidity_router contract" \
"## Problem
\`slippage_protection\` is a standalone contract. It needs to be invoked by \`liquidity_router\` before any USDC→fiat conversion to enforce slippage bounds.

## Acceptance Criteria
- [ ] liquidity_router calls slippage_protection.check_slippage() before swap
- [ ] Integration test: router swap rejected on slippage breach
- [ ] Integration test: router swap succeeds within bounds
- [ ] Configurable per-merchant max slippage via merchant_registry metadata" \
"enhancement,P1,liquidity-router"

ci "[P1] Add Soroban contract ABI documentation (JSON schemas)" \
"## Problem
The NestJS backend calls these contracts but has no machine-readable ABI documentation. Developers must read Rust source to understand call signatures.

## Acceptance Criteria
- [ ] Run \`stellar contract inspect\` or generate ABI JSON for each contract
- [ ] \`docs/abi/\` directory with one JSON per contract
- [ ] README links to ABI docs
- [ ] CI step verifies ABI is up to date (regenerated on each build)" \
"documentation,P1,architecture"

ci "[P1] payment_escrow: add dispute escalation timeout" \
"## Problem
A dispute can remain in \`Disputed\` state indefinitely if the admin never calls \`resolve_dispute\`. There is no timeout after which funds are automatically released or refunded.

## Acceptance Criteria
- [ ] Add \`dispute_resolution_deadline: u32\` to \`PaymentEscrow\` struct (set when dispute raised)
- [ ] After deadline, anyone can call \`auto_resolve_dispute(payment_id)\`
- [ ] Default resolution: refund to customer (conservative)
- [ ] Admin can still resolve before deadline
- [ ] Unit tests for all timeout scenarios" \
"enhancement,P1,payment-escrow"

ci "[P1] Add Soroban contract monitoring — emit custom events for all state changes" \
"## Problem
Some state changes (e.g., \`set_registry\`, \`transfer_admin\`, \`set_kyc_status\`) don't emit events. The backend's Soroban event indexer cannot track these changes.

## Acceptance Criteria
- [ ] Audit all state-changing functions across 14 contracts
- [ ] Add \`env.events().publish()\` for every missing state change
- [ ] Event topic naming convention documented in README
- [ ] Backend event indexer updated to handle new event types" \
"enhancement,P1,architecture"

ci "[P1] Add contract upgrade test for payment_escrow preserving in-flight payments" \
"## Problem
When \`payment_escrow\` is upgraded via \`upgrade()\`, all \`persistent\` storage (individual payments) should survive. There is no test verifying this.

## Acceptance Criteria
- [ ] Test: create payment → upgrade contract → verify payment still accessible
- [ ] Test: release payment after upgrade succeeds
- [ ] Test: dispute after upgrade succeeds
- [ ] Document upgrade procedure in \`docs/escrow_upgrade_procedure.md\`" \
"testing,P1,payment-escrow"

ci "[P1] batch_payments: batch size limit of 20 is too low — increase with gas analysis" \
"## Problem
\`MAX_BATCH_SIZE = 20\` is an arbitrary limit. Based on Soroban instruction limits and WASM size, a higher batch size (50–100) may be feasible. The README mentions 50 as the backend batch limit.

## Acceptance Criteria
- [ ] Benchmark gas cost per batch item
- [ ] Determine safe maximum batch size within Soroban instruction limits
- [ ] Update \`MAX_BATCH_SIZE\` and document the analysis
- [ ] Add test at the new maximum batch size" \
"enhancement,P1,batch-payments"

ci "[P1] fee_distributor: implement and document fee recipient list" \
"## Problem
There is no documented or implemented list of fee recipients (platform treasury, liquidity provider, operational wallet). Fee distribution logic is a core financial component.

## Acceptance Criteria
- [ ] \`FeeRecipient\` struct: \`address: Address, share_bps: u32\`
- [ ] Constructor accepts list of recipients (must sum to 10000 bps)
- [ ] \`distribute(caller, amount)\` splits and transfers to each recipient
- [ ] Admin can update recipients with timelock
- [ ] Full unit test suite" \
"enhancement,P1,fee-distributor"

ci "[P1] Add contract security audit checklist" \
"## Problem
There is a \`docs/security/\` directory but no contract-specific audit checklist. Before mainnet launch, each contract needs a security review.

## Acceptance Criteria
- [ ] \`docs/security/contract-audit-checklist.md\`
- [ ] Checklist includes: re-entrancy, integer overflow, access control, storage exhaustion, event completeness
- [ ] One checklist per contract filled out
- [ ] External audit report referenced (or placeholder for upcoming audit)" \
"security,P1,architecture"

ci "[P1] Enforce WASM size budget in CI for all 14 contracts" \
"## Problem
Only \`make check-wasm-size\` exists as a manual check. CI should automatically fail if any contract WASM exceeds the 64KB limit.

## Acceptance Criteria
- [ ] CI workflow runs size check after WASM build
- [ ] Reports size of each contract in CI output
- [ ] Fails if any contract exceeds 64KB
- [ ] Size report added to PR as a comment" \
"devops,P1,architecture"

ci "[P1] liquidity_router: add slippage-protected swap test with real token mocks" \
"## Problem
liquidity_router has test_snapshots but no test verifying the full swap flow with token mocks and slippage_protection integration.

## Acceptance Criteria
- [ ] Mock USDC and fiat token contracts
- [ ] Test: swap succeeds within slippage tolerance
- [ ] Test: swap reverts if price moves beyond tolerance
- [ ] Test: fee deducted before swap
- [ ] Document expected liquidity source (AMM, order book, etc.)" \
"testing,P1,liquidity-router"

ci "[P1] Add tarpaulin coverage gate — fail CI if coverage drops below 80%" \
"## Problem
Coverage is measured via tarpaulin (referenced in CI) but there is no minimum threshold. Coverage can drop unnoticed.

## Acceptance Criteria
- [ ] \`--fail-under 80\` added to tarpaulin invocation in CI
- [ ] Coverage report published as CI artifact
- [ ] Coverage badge in README
- [ ] Per-contract coverage breakdown" \
"testing,P1,devops"

ci "[P1] Document mainnet deployment procedure and emergency rollback" \
"## Problem
\`deploy.yml\` deploys to mainnet on version tags but there is no documented rollback procedure if a mainnet deployment has a bug.

## Acceptance Criteria
- [ ] \`docs/deployment.md\` with pre-deploy checklist
- [ ] Rollback procedure: can WASM be downgraded? (Soroban upgrade is one-way; document mitigation)
- [ ] Emergency pause procedure using multisig_admin
- [ ] Contact list for on-call engineer during deployment" \
"documentation,P1,devops"

ci "[P1] payment_escrow: max TTL of 518400 ledgers is ~30 days — document and test maximum payment lifetime" \
"## Problem
MAX_TTL_LEDGERS = 518400 (roughly 30 days at 5s/ledger). This means a payment can sit in escrow for 30 days before expiry. This has compliance and capital efficiency implications.

## Acceptance Criteria
- [ ] Document MAX_TTL_LEDGERS rationale in code comment and README
- [ ] Add business logic test: payment with max TTL is still accessible at ledger sequence MAX_TTL_LEDGERS - 1
- [ ] Discuss: should max TTL be configurable by admin?
- [ ] Ensure Soroban persistent storage TTL (state archival) is set correctly" \
"documentation,P1,payment-escrow"

ci "[P1] Add Soroban state archival TTL extension logic" \
"## Problem
Soroban persistent storage entries expire after their TTL. For long-lived payments (up to 30 days), the storage TTL may expire before the payment expiry, making payment data inaccessible.

## Acceptance Criteria
- [ ] Extend storage TTL on every read (bump TTL to \`current_ledger + MAX_TTL_LEDGERS\`)
- [ ] Add \`env.storage().persistent().extend_ttl()\` on every persistent get/set
- [ ] Test: payment accessible after TTL extension
- [ ] Document Soroban archival vs payment expiry difference" \
"bug,P1,payment-escrow"

ci "[P1] rbac_access: missing role hierarchy for ComplianceAdmin and OperationsAdmin" \
"## Problem
The role hierarchy (\`ReadOnly < ComplianceAdmin < OperationsAdmin < SuperAdmin\`) is implemented via \`role_rank\` but \`execute_operations_task\` and \`execute_compliance_task\` are stub functions with no actual business logic. The contract has no real callers.

## Acceptance Criteria
- [ ] Document which contracts and functions should use RBAC guards
- [ ] payment_escrow and settlement_ledger should check RBAC for admin operations instead of single admin address
- [ ] Integration test: OperationsAdmin can call release(); ComplianceAdmin cannot
- [ ] Unit test: ReadOnly can call read-only functions; cannot call mutations" \
"enhancement,P1,rbac-access"

ci "[P1] Add Soroban event indexer test — verify backend can parse all contract events" \
"## Problem
The NestJS backend's \`SorobanEventIndexerService\` parses contract events. If a contract emits an event with an unexpected XDR shape, the indexer silently drops it.

## Acceptance Criteria
- [ ] For each contract event type, add a test that serialises the event to XDR and deserialises it using the Stellar SDK
- [ ] CI runs these tests as part of the contract build
- [ ] Document event XDR schemas in \`docs/events.md\`" \
"testing,P1,architecture"

ci "[P1] Add multi-currency settlement support to settlement_ledger" \
"## Problem
\`SettlementRecord\` has \`amount\`, \`fee\`, \`net\` but no currency field. As the platform expands beyond NGN, the settlement ledger needs to record which fiat currency was used.

## Acceptance Criteria
- [ ] Add \`fiat_currency: String\` to \`SettlementRecord\`
- [ ] \`record_settlement\` accepts \`fiat_currency\` parameter
- [ ] Add \`list_settlements_by_currency\` query
- [ ] Migration test: existing records treated as NGN" \
"enhancement,P1,settlement-ledger"

# ── P2 MEDIUM (45) ───────────────────────────────────────────────────────────

ci "[P2] fee_calculator: add getter for current fee tier for a given merchant" \
"## Problem
There is no way to query what fee tier a merchant will be charged without actually calling \`calculate_fee\` (which also writes state). A read-only preview function is needed.

## Acceptance Criteria
- [ ] \`preview_fee(merchant, amount) -> (fee, net, bps)\` — read-only, no state change
- [ ] Used by frontend to show fee preview before payment
- [ ] Unit tests matching \`calculate_fee\` output" \
"enhancement,P2,fee-calculator"

ci "[P2] merchant_registry: add merchant count and pagination query" \
"## Problem
There is no way to list all registered merchants or get a count. Admin tools need this for oversight.

## Acceptance Criteria
- [ ] Maintain \`merchant_index: Vec<Address>\` in persistent storage
- [ ] \`merchant_count(env) -> u32\` view function
- [ ] \`list_merchants(env, page, page_size) -> Vec<MerchantRecord>\`
- [ ] Pagination with 0-indexed pages" \
"enhancement,P2,merchant-registry"

ci "[P2] Add \`cargo clippy\` to CI — enforce Rust lint standards" \
"## Problem
CI runs \`cargo test\` and \`cargo build\` but not \`cargo clippy\`. Clippy catches common Rust mistakes and enforces idiomatic patterns.

## Acceptance Criteria
- [ ] Add \`cargo clippy -- -D warnings\` to CI
- [ ] Fix all existing clippy warnings
- [ ] Add \`#[allow(clippy::...)\` only where justified with a comment
- [ ] Pre-push hook runs clippy locally" \
"quality,P2,architecture"

ci "[P2] Add \`cargo fmt --check\` to CI — enforce formatting" \
"## Problem
No automated Rust formatting check is in CI. Code style is inconsistent across contracts.

## Acceptance Criteria
- [ ] Add \`cargo fmt -- --check\` to CI
- [ ] Run \`cargo fmt\` to fix all existing formatting issues
- [ ] \`rustfmt.toml\` with project-specific settings
- [ ] Pre-commit hook runs \`cargo fmt\`" \
"quality,P2,architecture"

ci "[P2] batch_payments: add per-merchant batch rate limiting" \
"## Problem
Any merchant can call \`create_batch\` unlimited times in the same ledger. Without rate limiting, a buggy integration could create thousands of batch payments per block.

## Acceptance Criteria
- [ ] Track per-merchant batch count in current ledger
- [ ] Panic if merchant exceeds MAX_BATCHES_PER_LEDGER (e.g., 5)
- [ ] Counter resets at next ledger
- [ ] Unit test: 6th batch in same ledger fails" \
"security,P2,batch-payments"

ci "[P2] payment_escrow: add \`get_payments_by_merchant\` index" \
"## Problem
Payments are keyed by payment_id only. There is no index to look up all escrow payments for a given merchant. This is needed for audit and dispute overview.

## Acceptance Criteria
- [ ] Maintain \`DataKey::MerchantPayments(Address) -> Vec<BytesN<32>>\`
- [ ] Append payment_id to index on deposit
- [ ] \`list_payments_by_merchant(env, merchant, page) -> Vec<PaymentEscrow>\`
- [ ] Update index on release/expire/refund (or keep all statuses and filter client-side)" \
"enhancement,P2,payment-escrow"

ci "[P2] Add contract property-based tests using Soroban test environment" \
"## Problem
Unit tests cover specific cases. Property-based testing (fuzzing) would find edge cases in fee calculation, slippage math, and escrow arithmetic.

## Acceptance Criteria
- [ ] Add \`proptest\` or \`quickcheck\` as dev-dependency
- [ ] Property test: fee_calculator fee + net always equals amount
- [ ] Property test: slippage_protection bps calculation is always in [0, 10000]
- [ ] Property test: escrow remaining_amount is always non-negative
- [ ] Run in CI" \
"testing,P2,architecture"

ci "[P2] Document Soroban storage limits and TTL strategy per contract" \
"## Problem
Different contracts use instance vs persistent storage without a documented rationale. Incorrect storage type choices lead to data loss or cost overruns.

## Acceptance Criteria
- [ ] \`docs/storage-strategy.md\` explaining instance vs persistent vs temporary
- [ ] Document each contract's storage usage with justification
- [ ] Estimate storage cost per payment at current Stellar fee schedule
- [ ] TTL extension strategy for each contract" \
"documentation,P2,architecture"

ci "[P2] Add benchmark tests for gas cost of critical contract operations" \
"## Problem
There are no benchmarks for gas (instruction) cost of deposit, release, create_batch, or calculate_fee. Gas cost spikes after code changes would go unnoticed.

## Acceptance Criteria
- [ ] Add \`cargo test -- --include-ignored bench\` with gas measurement
- [ ] Benchmark: payment_escrow deposit, release, expire
- [ ] Benchmark: fee_calculator calculate_fee with 10-tier config
- [ ] Benchmark: batch_payments create_batch at max size
- [ ] Document expected gas costs in README" \
"performance,P2,architecture"

ci "[P2] multisig_admin: implement \`execute_with_quorum\` generic action dispatch" \
"## Problem
The multisig_admin contract appears to be a stub. It needs a generic action dispatch mechanism where M-of-N signers must approve an action before it executes.

## Acceptance Criteria
- [ ] \`propose_action(caller, action_type, target_contract, calldata)\` — signer 1 proposes
- [ ] \`approve_action(caller, action_id)\` — signers 2+ approve
- [ ] \`execute_action(caller, action_id)\` — any signer executes after quorum reached
- [ ] Action expires after configurable TTL
- [ ] Full unit test suite with 2-of-3 and 3-of-5 scenarios" \
"enhancement,P2,multisig-admin"

ci "[P2] Add Makefile targets for per-contract test and build" \
"## Problem
\`make test\` runs all tests. Developers working on a single contract must run the full suite. Per-contract targets speed up iteration.

## Acceptance Criteria
- [ ] \`make test-escrow\`, \`make test-registry\`, etc. for each contract
- [ ] \`make build-escrow\` builds single contract
- [ ] \`make clean\` removes all build artifacts
- [ ] \`make help\` lists all available targets" \
"devops,P2,architecture"

ci "[P2] Improve Grafana payment volume dashboard — add Soroban contract event lag metric" \
"## Problem
\`grafana/dashboards/payment-volume.json\` exists but does not include Soroban event indexing lag. If the backend falls behind on indexing events, payments could appear delayed.

## Acceptance Criteria
- [ ] Add \`soroban_event_lag_ledgers\` metric in Grafana dashboard
- [ ] Alert when lag > 10 ledgers
- [ ] Add panel showing last indexed ledger vs current ledger
- [ ] Update \`monitoring/README.md\`" \
"observability,P2,monitoring"

ci "[P2] Add payment_escrow fuzz test for deposit amount boundaries" \
"## Problem
Escrow arithmetic uses \`i128\` but no fuzz test verifies behaviour at boundary values (\`i128::MAX\`, negative values, zero, near-overflow sums).

## Acceptance Criteria
- [ ] Fuzz test: deposit with amount near i128::MAX
- [ ] Fuzz test: partial release summing to i128::MAX
- [ ] Fuzz test: released_amount overflow from multiple partial releases
- [ ] All boundary cases handled without panic (return error)" \
"testing,P2,payment-escrow"

ci "[P2] Add Docker-based local Soroban testnet environment" \
"## Problem
Developers must use the public Stellar testnet for integration testing. A local Soroban node via Docker would allow faster, isolated testing.

## Acceptance Criteria
- [ ] \`docker-compose.yml\` includes \`stellar/quickstart\` container
- [ ] \`make local-testnet-start\` and \`make local-testnet-stop\` targets
- [ ] Contract deployment script against local testnet
- [ ] README section: Running a local Soroban testnet" \
"devops,P2,architecture"

ci "[P2] Add \`stellar contract bindings\` generation to CI" \
"## Problem
The NestJS backend must manually maintain TypeScript types for contract calls. Stellar CLI can auto-generate TypeScript bindings from contract WASM.

## Acceptance Criteria
- [ ] CI step generates TypeScript bindings after build
- [ ] Bindings published as npm package or GitHub release asset
- [ ] Backend imports auto-generated types instead of hand-written ones
- [ ] Bindings regenerated on every merge to main" \
"devops,P2,architecture"

ci "[P2] admin_timelock: add minimum and maximum delay validation" \
"## Problem
The timelock delay should have a minimum (to prevent bypassing the timelock) and a maximum (to prevent admin being locked out). Neither bound is enforced.

## Acceptance Criteria
- [ ] MIN_DELAY: 24 hours
- [ ] MAX_DELAY: 30 days
- [ ] Constructor and set_delay() validate against bounds
- [ ] Unit tests for boundary conditions" \
"security,P2,admin-timelock"

ci "[P2] payment_escrow: verify merchant is KYC-verified before allowing deposit" \
"## Problem
The escrow only checks \`is_approved\` (active status) via the registry. It does not check \`kyc_verified\`. Payments to non-KYC merchants create compliance risk.

## Acceptance Criteria
- [ ] Add \`is_kyc_verified\` check in deposit() if registry is configured
- [ ] Admin-configurable \`require_kyc\` flag on escrow constructor
- [ ] Unit test: deposit to non-KYC merchant fails when require_kyc=true
- [ ] Unit test: deposit to non-KYC merchant succeeds when require_kyc=false" \
"compliance,P2,payment-escrow"

ci "[P2] Update \`docs/escrow_upgrade_procedure.md\` with upgrade sequence for all 14 contracts" \
"## Problem
The upgrade procedure document only covers the escrow contract. All 14 contracts have different interdependencies and require a specific upgrade order.

## Acceptance Criteria
- [ ] Document upgrade order (dependency graph)
- [ ] Indicate which contracts require timelock before upgrade
- [ ] Indicate which upgrades require multisig quorum
- [ ] Rollback notes per contract
- [ ] Test the procedure end-to-end on testnet" \
"documentation,P2,architecture"

ci "[P2] Add \`codecov.yml\` configuration to enforce per-package coverage" \
"## Problem
\`codecov.yml\` exists but may not be configured to enforce per-contract coverage thresholds. A new contract with 0% coverage can be merged.

## Acceptance Criteria
- [ ] Set per-component coverage threshold to 80% in codecov.yml
- [ ] PR fails if any contract drops below threshold
- [ ] Coverage badge per contract in README
- [ ] Exclude test files from coverage calculation" \
"quality,P2,architecture"

ci "[P2] settlement_ledger: add settlement reversal record type" \
"## Problem
Once a settlement is recorded it's immutable, but the backend may need to record a reversal (chargeback). There is no reversal record type.

## Acceptance Criteria
- [ ] \`SettlementReversal\` struct referencing original payment_id
- [ ] \`record_reversal(caller, original_payment_id, reason)\` — admin only
- [ ] Cannot reverse an already-reversed settlement
- [ ] Reversal emits event and updates merchant index" \
"enhancement,P2,settlement-ledger"

ci "[P2] Add Dependabot / cargo audit for Rust dependency vulnerability scanning" \
"## Problem
Soroban SDK and other Rust dependencies may have known vulnerabilities. No automated dependency audit runs in CI.

## Acceptance Criteria
- [ ] \`.github/dependabot.yml\` for cargo dependencies
- [ ] \`cargo audit\` step in CI (via \`cargo-audit\` crate)
- [ ] Fail CI on HIGH or CRITICAL severity vulnerabilities
- [ ] Weekly dependency audit report (workflow already exists: \`weekly-dependency-audit-report.yml\`)" \
"security,P2,devops"

ci "[P2] payment_escrow: add support for partial dispute resolution" \
"## Problem
\`resolve_dispute\` sends all remaining funds to the winner. There is no way to resolve a dispute with a partial split (e.g., 70% to merchant, 30% refund to customer).

## Acceptance Criteria
- [ ] \`resolve_dispute_split(caller, payment_id, merchant_amount, customer_amount)\`
- [ ] Validates \`merchant_amount + customer_amount == remaining\`
- [ ] Emits separate release and refund events
- [ ] Unit tests for various split ratios" \
"enhancement,P2,payment-escrow"

ci "[P2] Add contract interaction diagram to README" \
"## Problem
With 14 contracts that call each other, the dependency graph is hard to visualise from code alone. A Mermaid diagram in README would help contributors and auditors.

## Acceptance Criteria
- [ ] Mermaid diagram: contract call graph
- [ ] Separate diagram: data flow for payment_escrow deposit → settlement_ledger record
- [ ] Rendered in GitHub README
- [ ] Updated when new cross-contract calls are added" \
"documentation,P2,architecture"

ci "[P2] Improve monitoring/statuspage with Soroban-specific health checks" \
"## Problem
The statuspage (\`monitoring/statuspage/\`) shows generic service health. Soroban-specific checks (contract reachability, event indexer lag) are missing.

## Acceptance Criteria
- [ ] Status check: Soroban RPC endpoint response time
- [ ] Status check: last payment_escrow event indexed within 30s
- [ ] Status check: fee_calculator returns valid fee for test merchant
- [ ] Update \`monitoring/statuspage/worker.js\`" \
"observability,P2,monitoring"

ci "[P2] slippage_protection: add support for both XLM/USDC and USDC/fiat pairs" \
"## Problem
Slippage check uses a generic \`expected\` vs \`actual\` price. There is no pair-specific tolerance (XLM/USDC may need tighter slippage than USDC/NGN).

## Acceptance Criteria
- [ ] \`check_slippage_for_pair(pair_id, expected, actual)\`
- [ ] Per-pair max slippage configured by admin
- [ ] Default pair max slippage falls back to global max
- [ ] Unit tests for XLM/USDC and USDC/NGN pairs" \
"enhancement,P2,slippage-protection"

ci "[P2] Add weekly-dependency-audit-report workflow to post results to Slack/GitHub Issues" \
"## Problem
\`weekly-dependency-audit-report.yml\` runs but the output is only visible in GitHub Actions. Vulnerabilities may be missed if no one checks the Actions tab.

## Acceptance Criteria
- [ ] Post audit results as a GitHub Issue (labelled \`security\`) when vulnerabilities found
- [ ] Or post to Slack security channel via webhook
- [ ] Auto-close the issue when dependencies are updated and audit passes
- [ ] Include CVE links in the report" \
"security,P2,devops"

ci "[P2] payment_escrow: add \`is_expired()\` view function" \
"## Problem
External callers (backend, indexer) must call \`get_payment\` and manually compare expiry to current ledger. A convenience view function reduces off-chain computation.

## Acceptance Criteria
- [ ] \`is_expired(env, payment_id) -> bool\`
- [ ] Returns true if current ledger > payment.expiry AND status is Pending
- [ ] Used by backend instead of manual comparison
- [ ] Unit tests" \
"enhancement,P2,payment-escrow"

ci "[P2] Add Soroban-specific CI optimizations — cache cargo registry and target" \
"## Problem
CI builds take long because Rust re-downloads and recompiles dependencies on every run. Cargo registry and target directory should be cached.

## Acceptance Criteria
- [ ] \`actions/cache\` for \`~/.cargo/registry\` and \`target/\`
- [ ] Cache key includes Cargo.lock hash
- [ ] Measure build time improvement with and without cache
- [ ] Document expected CI build time in README" \
"devops,P2,architecture"

ci "[P2] Add gas optimization pass for fee_calculator tiers loop" \
"## Problem
\`select_fee_bps\` iterates over all fee tiers on every \`calculate_fee\` call. With many tiers, this is O(n) in instruction count. Binary search would be O(log n).

## Acceptance Criteria
- [ ] Replace linear scan with binary search on sorted \`threshold_usdc\`
- [ ] Tiers are already validated as strictly increasing (sort guarantee)
- [ ] Benchmark: instruction count before and after with 20-tier config
- [ ] Existing tests pass after optimization" \
"performance,P2,fee-calculator"

ci "[P2] Document all storage DataKey variants and their types for each contract" \
"## Problem
Each contract defines a \`DataKey\` enum but the types stored at each key are only known by reading the code. A documentation table would help auditors and integrators.

## Acceptance Criteria
- [ ] \`docs/storage-keys.md\` table: contract | key | storage type | value type | TTL
- [ ] One row per DataKey variant
- [ ] Notes on which keys are set in constructor vs runtime
- [ ] Updated when new keys are added" \
"documentation,P2,architecture"

ci "[P2] Add integration test runner script for full payment lifecycle" \
"## Problem
There is no script that deploys all contracts to testnet and runs through the complete payment_request → payment_escrow deposit → fee_calculator → settlement_ledger lifecycle.

## Acceptance Criteria
- [ ] \`scripts/integration-test.sh\` or a Rust integration test binary
- [ ] Steps: deploy all contracts, register merchant, create escrow deposit, release, record settlement
- [ ] Prints pass/fail for each step with transaction hashes
- [ ] Runs against local testnet from docker-compose" \
"testing,P2,architecture"

# ── P3 LOW (35) ──────────────────────────────────────────────────────────────

ci "[P3] Add \`#![deny(warnings)]\` to all contract crates" \
"## Problem
Rust compiler warnings may exist in contracts and are not treated as errors. Warnings can mask real issues.

## Acceptance Criteria
- [ ] Add \`#![deny(warnings)]\` to each contract's \`lib.rs\`
- [ ] Fix all existing warnings
- [ ] CI builds with \`RUSTFLAGS=\"-D warnings\"\`
- [ ] Document any intentional \`#[allow(...)]\` usages" \
"quality,P3,architecture"

ci "[P3] Add \`CHANGELOG.md\` tracking contract version history" \
"## Problem
No changelog exists for contract versions. After upgrades, it is unclear what changed between versions.

## Acceptance Criteria
- [ ] \`CHANGELOG.md\` in repo root using Keep a Changelog format
- [ ] Entries include: contract name, version bump, what changed, breaking changes
- [ ] Reference to deployment transaction hash on testnet/mainnet
- [ ] Process for updating changelog on every merge to main" \
"documentation,P3,architecture"

ci "[P3] Add contract README for each of the 14 contracts" \
"## Problem
Only the repo-level README exists. Individual contracts have no documentation on their purpose, API, security model, or known limitations.

## Acceptance Criteria
- [ ] \`contracts/<name>/README.md\` for each contract
- [ ] Sections: Purpose, Storage, Events, Admin operations, Known limitations
- [ ] Cross-references to dependent contracts" \
"documentation,P3,architecture"

ci "[P3] Standardise error messages across all contracts" \
"## Problem
Error messages are inconsistent: some use \`panic!(\"Not admin\")\`, others \`panic!(\"not admin\")\`, others \`assert!(condition, \"not admin\")\`. This makes error detection in the backend unreliable.

## Acceptance Criteria
- [ ] Agree on error message format: PascalCase code (e.g., \`NotAdmin\`, \`PaymentNotFound\`)
- [ ] Once \`#[contracterror]\` enums are adopted (see P0 issue), this is resolved
- [ ] Until then, standardise panic strings using a shared constants module" \
"quality,P3,architecture"

ci "[P3] Add Mermaid sequence diagram for dispute resolution flow" \
"## Problem
The dispute → resolve_dispute flow involves multiple parties (customer, merchant, admin) and is complex enough to warrant a sequence diagram.

## Acceptance Criteria
- [ ] Mermaid sequence diagram in \`contracts/payment_escrow/README.md\`
- [ ] Covers: dispute raised, admin reviews, resolved to merchant, resolved to customer, auto-resolved on timeout
- [ ] Rendered in GitHub" \
"documentation,P3,payment-escrow"

ci "[P3] Add Soroban testnet deployment verification script" \
"## Problem
After deployment, there is no automated check that the deployed contracts respond correctly to basic queries.

## Acceptance Criteria
- [ ] \`scripts/verify-deployment.sh\` — calls \`get_admin\`, \`get_version\`, and basic read queries on each contract
- [ ] Prints OK / FAIL for each contract
- [ ] Exits with non-zero code if any contract fails
- [ ] Run automatically after deploy workflow" \
"devops,P3,architecture"

ci "[P3] Rename \`dabdub_contracts\` workspace directory to \`dupdub_contracts\`" \
"## Problem
The workspace directory is named \`dabdub_contracts\` (old brand name) while the repo and project are rebranded to DupDub. This is inconsistent.

## Acceptance Criteria
- [ ] Rename directory from \`dabdub_contracts\` to \`dupdub_contracts\`
- [ ] Update all references in Makefile, CI workflows, README
- [ ] Update deploy.yml paths
- [ ] Confirm \`cargo test\` still passes after rename" \
"quality,P3,architecture"

ci "[P3] Add contract ABI version field to all constructors" \
"## Problem
Contracts have a \`Version\` storage key in payment_escrow but not all contracts track their version. Version tracking is needed for the backend to know which ABI to use.

## Acceptance Criteria
- [ ] Add \`DataKey::Version\` and initial value \`1u32\` to all contracts that don't have it
- [ ] \`get_version(env) -> u32\` on all contracts
- [ ] Version bumped on every WASM upgrade
- [ ] CI checks that version constant matches Cargo.toml version" \
"enhancement,P3,architecture"

ci "[P3] Add \`pci-scope.md\` review notes for Soroban contracts" \
"## Problem
\`docs/security/pci-scope.md\` exists but likely doesn't cover Soroban smart contract scope. Payment escrow contracts handle cardholder funds and need PCI scoping.

## Acceptance Criteria
- [ ] Review and update \`pci-scope.md\` to include contracts
- [ ] Classify each contract: in-scope, out-of-scope, or N/A for PCI
- [ ] List controls applied at the smart contract layer
- [ ] Date of last review" \
"compliance,P3,security"

ci "[P3] Add contract deployment address book to repository" \
"## Problem
Testnet and mainnet contract IDs are not tracked in the repository. Developers must ask teammates or check deployment logs.

## Acceptance Criteria
- [ ] \`deployments/testnet.json\` — contract IDs by name
- [ ] \`deployments/mainnet.json\` — populated on first mainnet deploy
- [ ] Deploy workflow updates these files and commits them
- [ ] README links to deployment files" \
"documentation,P3,devops"

ci "[P3] Add Soroban contract size report to PR comments" \
"## Problem
WASM size changes are invisible in PR reviews. A size regression is only caught at deploy time.

## Acceptance Criteria
- [ ] CI step measures each contract WASM size
- [ ] Posts a table to the PR as a GitHub comment: contract | current size | previous size | delta
- [ ] Highlights contracts that exceed 80% of 64KB budget in yellow
- [ ] Highlights contracts over budget in red" \
"devops,P3,architecture"

ci "[P3] Add \`make lint\` target combining clippy, fmt, and audit" \
"## Problem
Developers run \`cargo clippy\`, \`cargo fmt --check\`, and \`cargo audit\` separately. A single \`make lint\` reduces friction.

## Acceptance Criteria
- [ ] \`make lint\` runs clippy, fmt check, and audit in sequence
- [ ] Stops on first failure with clear error message
- [ ] Listed in README under Development section
- [ ] Pre-push hook calls \`make lint\`" \
"quality,P3,architecture"

ci "[P3] Add Grafana alert rule for emergency_drain event" \
"## Problem
An \`emergency_drain\` event on payment_escrow is a critical security event that should page on-call immediately.

## Acceptance Criteria
- [ ] Grafana alert rule triggers on any \`ESCROW.emergency_drain\` event in Loki
- [ ] Alert fires within 1 minute of event
- [ ] Pages PagerDuty and Slack #security channel
- [ ] Runbook link in alert" \
"security,P3,monitoring"

ci "[P3] Add \`review-schedule.md\` quarterly review process for contract security" \
"## Problem
\`docs/security/review-schedule.md\` exists but may not include smart contract-specific review items.

## Acceptance Criteria
- [ ] Add contract review items: storage exhaustion checks, fee tier accuracy, role assignments
- [ ] Quarterly review checklist for each contract
- [ ] Action items logged as GitHub Issues after each review
- [ ] External audit schedule documented" \
"security,P3,security"

ci "[P3] Add \`vulnerability-management.md\` process for Soroban contract CVEs" \
"## Problem
\`docs/security/vulnerability-management.md\` exists for general vulnerabilities but needs a section specific to smart contract vulnerabilities (which cannot be patched without a contract upgrade).

## Acceptance Criteria
- [ ] Add section: Smart Contract Vulnerability Response Process
- [ ] Steps: identify → assess → pause contract (if critical) → deploy fix → disclose
- [ ] Severity matrix: Critical (drain possible), High (funds frozen), Medium (incorrect fees)
- [ ] Communication template for merchant notification" \
"security,P3,security"

ci "[P3] Add GitHub issue templates for bug reports and feature requests" \
"## Problem
No GitHub issue templates exist. Bug reports lack reproducible steps, and feature requests lack context.

## Acceptance Criteria
- [ ] \`.github/ISSUE_TEMPLATE/bug_report.md\` — with contract name, expected vs actual, test case
- [ ] \`.github/ISSUE_TEMPLATE/feature_request.md\` — with motivation and Acceptance Criteria format
- [ ] \`.github/PULL_REQUEST_TEMPLATE.md\` — with test coverage checkbox and CHANGELOG update
- [ ] Config to select template on issue creation" \
"documentation,P3,architecture"

echo "Done! 125 issues created for dupdapp_stellar."
