#!/usr/bin/env bash
set -e
REPO="dupdab/dupdapp-stellar"

i() {
  gh issue create --repo "$REPO" --title "$1" --body "$2"
  sleep 0.3
}

echo "=== Creating 125 Stellar contract issues ==="

i "payment_escrow: suspended merchant can still receive payout via release() after deposit" "A merchant suspended after a customer's deposit can still have \`release()\` or \`release_partial()\` called on their behalf. The contract does not check merchant suspension status at release time.

**Acceptance criteria:**
- [ ] \`release()\` checks merchant registry status before transferring funds
- [ ] Returns error code if merchant is suspended at time of release
- [ ] Funds remain in escrow pending admin decision
- [ ] Unit test: suspend merchant post-deposit → release returns error"

i "payment_escrow: dispute _reason parameter underscore-prefixed despite being used" "The \`_reason\` parameter in \`dispute()\` is prefixed with underscore (convention for unused params in Rust) but is actually used in the function body, making it misleading.

**Acceptance criteria:**
- [ ] Rename to \`reason\` throughout the function
- [ ] No other underscore-prefixed params that are actually used
- [ ] CI lint check for unused-variable false positives"

i "payment_escrow: dispute resolution is all-or-nothing on remainder after partial release" "If a merchant has already called \`release_partial()\`, a subsequent dispute resolution awards the full remaining escrow amount without accounting for how much was already legitimately released.

**Acceptance criteria:**
- [ ] Dispute resolution should only act on the disputed portion
- [ ] Track disputed_amount separately from total_amount in escrow state
- [ ] Unit test: partial release → dispute → resolution only covers remainder"

i "payment_escrow: dispute reason parameter has no length or content validation" "The \`reason\` string passed to \`dispute()\` has no maximum length validation. An arbitrarily large string can be stored in contract state, inflating ledger entry sizes.

**Acceptance criteria:**
- [ ] Validate reason length <= 256 bytes
- [ ] Return \`Error::InvalidReason\` on oversized input
- [ ] Fuzz test with large reason strings"

i "payment_escrow: upgrade() function has thin test coverage" "\`upgrade()\` replaces the contract WASM. It is the highest-risk operation and has minimal test coverage.

**Acceptance criteria:**
- [ ] Test: upgrade succeeds with valid admin auth
- [ ] Test: upgrade fails without admin auth
- [ ] Test: state is preserved after upgrade
- [ ] Test: upgrade to malformed WASM fails gracefully"

i "payment_escrow: emergency_drain XLM path has thin test coverage" "\`emergency_drain()\` has a code path for draining native XLM balances. This path has minimal tests.

**Acceptance criteria:**
- [ ] Test: drain XLM to specified address
- [ ] Test: drain fails without admin auth
- [ ] Test: drain fails when contract has zero XLM balance
- [ ] Test: drain emits correct event"

i "payment_escrow: set_registry(None) has thin test coverage" "Setting registry to None effectively disables merchant validation. This high-risk operation has minimal coverage.

**Acceptance criteria:**
- [ ] Test: set_registry(None) succeeds with admin auth
- [ ] Test: after set_registry(None), release() behavior documented and tested
- [ ] Test: set_registry(None) fails without admin auth"

i "payment_escrow: no reentrancy guard on deposit() and release()" "Soroban contracts can call other contracts. If the USDC token contract is malicious or compromised, it could re-enter \`deposit()\` or \`release()\` before state is updated.

**Acceptance criteria:**
- [ ] Add a reentrancy guard flag in contract storage
- [ ] Set flag to true at function entry, false at exit
- [ ] Return \`Error::Reentrant\` if flag is already set
- [ ] Unit test simulating reentrant call"

i "payment_escrow: expiry check uses ledger sequence — no handling for ledger slowdown" "Payment expiry is checked against ledger sequence numbers (expiryLedger). If Stellar network slows down (ledger close time > 5s), the actual time-to-expiry drifts.

**Acceptance criteria:**
- [ ] Document the ledger-time relationship assumption
- [ ] Add a maximum ledger count per payment (e.g. 17280 = ~24h at 5s/ledger)
- [ ] Backend and contract must agree on ledgers-per-minute constant"

i "payment_escrow: no event emitted on payment expiry — backend cannot detect expired escrows" "When a payment expires in the escrow contract, no event is emitted. The backend has no on-chain signal to transition the payment to EXPIRED status.

**Acceptance criteria:**
- [ ] Emit \`PaymentExpired { payment_id, merchant_id, amount }\` event when expiry is detected
- [ ] Backend Soroban event indexer listens for and processes this event
- [ ] Unit test for expiry event emission"

i "payment_escrow: release() does not validate that the payment belongs to the calling merchant" "\`release()\` takes a payment_id and recipient. If not properly guarded, one merchant could call release on another merchant's payment.

**Acceptance criteria:**
- [ ] Verify \`payment.merchant_id == caller\` before release
- [ ] Return \`Error::Unauthorized\` on mismatch
- [ ] Unit test for cross-merchant release attempt"

i "payment_escrow: no maximum payment amount enforced at contract level" "The contract accepts any USDC amount. Without a cap, a single oversized payment could exhaust platform liquidity or trigger contract size limits.

**Acceptance criteria:**
- [ ] Admin-configurable max_payment_amount in contract storage
- [ ] \`deposit()\` returns \`Error::AmountExceedsMaximum\` if exceeded
- [ ] Admin function to update the cap
- [ ] Unit test for cap enforcement"

i "payment_escrow: partial release does not emit event with remaining amount" "\`release_partial()\` emits an event but may not include the remaining_amount field, making it hard for the backend to reconcile without a separate state query.

**Acceptance criteria:**
- [ ] \`PartialReleaseEvent\` includes: payment_id, released_amount, remaining_amount
- [ ] Backend event indexer parses remaining_amount
- [ ] Unit test asserting event fields"

i "payment_escrow: no cancel function for merchant to cancel before customer deposits" "Once a payment request is registered in the contract, there is no way for the merchant to cancel it before the customer deposits (e.g. if the customer abandons checkout).

**Acceptance criteria:**
- [ ] \`cancel(payment_id)\` callable by merchant before any deposit
- [ ] Returns \`Error::AlreadyDeposited\` if deposit exists
- [ ] Emits \`PaymentCancelled\` event
- [ ] Unit test for cancel before and after deposit"

i "payment_escrow: escrow state not cleaned up after settlement — ledger bloat over time" "After a payment is fully released or refunded, the escrow entry remains in contract storage indefinitely, consuming ledger rent.

**Acceptance criteria:**
- [ ] Delete escrow entry from storage after final release or refund
- [ ] Emit \`EscrowClosed\` event on deletion
- [ ] Benchmark ledger entry count before/after cleanup
- [ ] Unit test asserting storage cleared post-settlement"

i "merchant_registry: no event emitted on merchant registration — backend cannot index registrations" "When a merchant registers via \`merchant_registry\`, no event is emitted. The backend must poll for new registrations instead of reacting to events.

**Acceptance criteria:**
- [ ] Emit \`MerchantRegistered { merchant_id, public_key, timestamp }\` event
- [ ] Backend event indexer subscribes to these events
- [ ] Unit test for event emission"

i "merchant_registry: no event emitted on merchant suspension — payment_escrow release proceeds" "When a merchant is suspended in the registry, no event is emitted. The escrow contract cannot proactively freeze pending payments.

**Acceptance criteria:**
- [ ] Emit \`MerchantSuspended { merchant_id, reason, suspended_at }\` event
- [ ] Backend freezes pending payments on receipt
- [ ] Unit test for suspension event"

i "merchant_registry: merchant metadata has no max size validation — unbounded ledger entries" "Merchant metadata stored in the registry (business name, country, etc.) has no size validation, potentially creating very large ledger entries.

**Acceptance criteria:**
- [ ] Max 1KB for all merchant metadata fields combined
- [ ] Return \`Error::MetadataTooLarge\` on exceeded
- [ ] Fuzz test with large metadata payloads"

i "merchant_registry: no way to transfer merchant ownership — public key locked forever" "The merchant's Stellar public key is set at registration and cannot be updated. If a merchant loses their key, their account is permanently inaccessible.

**Acceptance criteria:**
- [ ] \`transfer_ownership(new_public_key)\` function callable by current owner
- [ ] Admin can override in emergency
- [ ] Emits \`OwnershipTransferred\` event
- [ ] 48h timelock on ownership transfer"

i "merchant_registry: get_merchant() returns all fields including sensitive ones — no field filtering" "\`get_merchant()\` returns the full merchant struct including fields that may be sensitive (e.g. internal flags, admin notes).

**Acceptance criteria:**
- [ ] Public \`get_merchant()\` returns a filtered PublicMerchant view
- [ ] Admin-only \`get_merchant_full()\` returns all fields
- [ ] Unit test asserting sensitive fields not in public response"

i "fee_calculator: fee rate not validated against configured bounds at calculation time" "The fee calculator applies a fee rate but may not validate it against minimum/maximum bounds at calculation time, potentially charging 0% or 100%.

**Acceptance criteria:**
- [ ] Validate fee_rate is within [min_fee_rate, max_fee_rate] bounds
- [ ] Return \`Error::InvalidFeeRate\` on out-of-bounds
- [ ] Admin function to update bounds
- [ ] Unit test for boundary values"

i "fee_calculator: integer overflow risk on large USDC amounts in fee computation" "Fee calculation multiplies amount by rate using integer arithmetic. For very large amounts, this may overflow before the division step.

**Acceptance criteria:**
- [ ] Use checked_mul and checked_div throughout
- [ ] Return \`Error::Overflow\` on detected overflow
- [ ] Unit test with max u128 amount"

i "fee_calculator: no minimum fee enforced — tiny payments generate sub-cent fees" "For very small payments (e.g. \$0.01), the percentage fee generates a sub-cent amount. The contract should enforce a minimum fee.

**Acceptance criteria:**
- [ ] Admin-configurable minimum_fee_stroops
- [ ] Fee returned is max(percentage_fee, minimum_fee)
- [ ] Unit test with payment below minimum threshold"

i "fee_calculator: fee calculation result not rounded consistently — rounding direction not documented" "The fee calculation may round up or down depending on integer division, potentially shortchanging the platform.

**Acceptance criteria:**
- [ ] Document rounding direction (round up = platform favored)
- [ ] Use ceiling division for fee calculation
- [ ] Unit test: 100 USDC at 1.5% = 1.5 USDC rounded to 2 USDC"

i "fee_distributor: distribution recipients not validated against an allowlist" "\`fee_distributor\` sends fees to configured recipient addresses without validating them against an admin-managed allowlist. A misconfigured recipient address permanently loses fees.

**Acceptance criteria:**
- [ ] Admin-managed recipient allowlist in storage
- [ ] Distribution fails if any recipient is not on the allowlist
- [ ] \`Error::InvalidRecipient\` returned on unknown recipient
- [ ] Unit test for allowlist enforcement"

i "fee_distributor: no event emitted per distribution — fee tracking requires state queries" "Fee distribution events are not emitted per transfer, forcing the backend to diff contract state to track fee flows.

**Acceptance criteria:**
- [ ] Emit \`FeeDistributed { recipient, amount, payment_id }\` per distribution
- [ ] Backend event indexer aggregates fee events
- [ ] Unit test for event emission"

i "fee_distributor: distribution shares sum not validated to equal 100% — lost fees possible" "The distribution shares (percentages per recipient) are not validated to sum to exactly 100%. Rounding errors or misconfigurations silently lose fees.

**Acceptance criteria:**
- [ ] Validate sum of shares == 10000 (basis points) on \`set_distribution()\`
- [ ] Return \`Error::SharesMustSum100Percent\` on mismatch
- [ ] Unit test for sum validation"

i "fee_distributor: no maximum number of recipients — unbounded distribution loop" "The contract may allow an unlimited number of fee recipients. With many recipients, the distribution loop could exceed Soroban instruction limits.

**Acceptance criteria:**
- [ ] Cap recipients at 10 (configurable by admin)
- [ ] Return \`Error::TooManyRecipients\` on exceeded
- [ ] Load test: 10 recipients within instruction budget"

i "liquidity_router: slippage tolerance not enforced at contract level — only at backend" "Slippage protection is handled by the backend before invoking the contract. The contract itself does not enforce a maximum slippage, allowing the backend to bypass protection.

**Acceptance criteria:**
- [ ] Accept \`max_slippage_bps\` parameter on route invocations
- [ ] Contract validates actual rate vs quoted rate before proceeding
- [ ] Return \`Error::SlippageExceeded\` on violation
- [ ] Unit test for slippage enforcement"

i "liquidity_router: no circuit breaker when multiple routes fail consecutively" "If multiple liquidity routes fail consecutively (e.g. DEX liquidity crisis), the router keeps retrying with no circuit breaker.

**Acceptance criteria:**
- [ ] Track consecutive failure count per route in storage
- [ ] Disable route after 5 consecutive failures
- [ ] Admin function to re-enable a route
- [ ] Alert emitted via event on circuit open"

i "liquidity_router: price oracle not validated for freshness — stale prices accepted" "The liquidity router consults a price oracle but does not validate the oracle timestamp. A stale oracle price could cause incorrect routing decisions.

**Acceptance criteria:**
- [ ] Validate oracle timestamp is within max_oracle_age_seconds of current ledger time
- [ ] Return \`Error::OraclePriceTooOld\` on stale price
- [ ] Admin-configurable max_oracle_age_seconds"

i "liquidity_router: no event on route selection — backend cannot audit which route was used" "When a route is selected for a conversion, no event is emitted with which route was chosen and at what rate.

**Acceptance criteria:**
- [ ] Emit \`RouteSelected { route_id, rate, amount_in, amount_out }\`
- [ ] Backend indexes route selection events for analytics
- [ ] Unit test for event emission"

i "settlement_ledger: no idempotency check on record_settlement() — duplicate entries possible" "If the backend retries a settlement recording call, duplicate entries may be created in the settlement ledger.

**Acceptance criteria:**
- [ ] Use settlement_id as idempotency key in storage
- [ ] Return existing entry on duplicate settlement_id
- [ ] Unit test for idempotent replay"

i "settlement_ledger: settlement records never deleted — unbounded ledger growth" "Settlement records are stored permanently. As settlement volume grows, the ledger data size grows without bound, increasing rent costs.

**Acceptance criteria:**
- [ ] Admin function to archive settlements older than N ledgers
- [ ] Archive by moving to a secondary storage key with TTL
- [ ] Emit \`SettlementArchived\` event per archived record
- [ ] Unit test for archival"

i "settlement_ledger: no query function for settlement history by merchant — backend must scan all entries" "No \`get_settlements_by_merchant(merchant_id)\` function exists. The backend must scan all settlement entries to find a merchant's history.

**Acceptance criteria:**
- [ ] Add merchant-indexed lookup in contract storage (vec of settlement_ids per merchant)
- [ ] \`get_merchant_settlements(merchant_id, limit, cursor)\` function
- [ ] Unit test for paginated merchant settlement query"

i "settlement_ledger: settlement amount not validated against payment escrow amount" "The settlement ledger records an amount that is not cross-validated against the actual escrow release amount. Backend bugs could record incorrect settlement amounts.

**Acceptance criteria:**
- [ ] Settlement ledger entry references the escrow payment_id
- [ ] Validate recorded_amount <= escrow release_amount
- [ ] Return \`Error::AmountMismatch\` on discrepancy
- [ ] Unit test for amount validation"

i "reconciliation: no reconciliation run event — backend cannot know when reconciliation completes" "When reconciliation runs, no event is emitted. The backend must poll contract state to detect completion.

**Acceptance criteria:**
- [ ] Emit \`ReconciliationCompleted { run_id, discrepancies_found, timestamp }\`
- [ ] Backend indexes reconciliation events
- [ ] Unit test for event emission"

i "reconciliation: discrepancy detection logic not tested with adversarial inputs" "The reconciliation contract compares on-chain state against expected values. Edge cases (zero amounts, identical IDs, overflows) are likely untested.

**Acceptance criteria:**
- [ ] Fuzz test: random payment amounts and statuses
- [ ] Test: reconciliation with duplicate payment IDs
- [ ] Test: reconciliation with zero-amount settlement
- [ ] Test: reconciliation with max u128 amount"

i "reconciliation: no mechanism to report discrepancies on-chain — only logs them" "When reconciliation finds a discrepancy, it logs but does not write a discrepancy record to storage for later audit.

**Acceptance criteria:**
- [ ] Store \`Discrepancy { payment_id, expected, actual, detected_at }\` in contract storage
- [ ] Admin function to query discrepancy history
- [ ] Emit \`DiscrepancyDetected\` event per finding
- [ ] Unit test for discrepancy storage"

i "reconciliation: reconciliation not protected against running during active settlements" "If reconciliation runs while settlements are in-flight, it will detect false discrepancies for payments that are legitimately mid-settlement.

**Acceptance criteria:**
- [ ] Check for in-flight settlements before running reconciliation
- [ ] Return \`Error::SettlementsInFlight\` if any are pending
- [ ] Or use a snapshot-based approach that doesn't block settlements
- [ ] Unit test for concurrent settlement + reconciliation"

i "batch_payments: no atomicity guarantee — partial batch commit possible on instruction limit hit" "If the \`batch_payments\` contract exceeds Soroban instruction limits mid-batch, some payments may be registered while others are not, leaving the batch in a partial state.

**Acceptance criteria:**
- [ ] Validate entire batch before committing any state changes
- [ ] Use all-or-nothing writes: fail entire batch if any item fails
- [ ] Unit test: batch with one invalid item fails entire batch
- [ ] Benchmark: max safe batch size within instruction limits"

i "batch_payments: no maximum batch size enforced at contract level" "The contract may accept arbitrarily large batches that exceed Soroban CPU/memory limits.

**Acceptance criteria:**
- [ ] Admin-configurable max_batch_size (default 20)
- [ ] Return \`Error::BatchTooLarge\` if exceeded
- [ ] Unit test for max size enforcement
- [ ] Benchmark at max size to verify within limits"

i "batch_payments: batch event does not include individual payment IDs — hard to correlate" "The batch creation event may only include a batch_id, not the individual payment_ids created. The backend cannot correlate batch events to individual payments.

**Acceptance criteria:**
- [ ] \`BatchCreated\` event includes vec of all payment_ids in the batch
- [ ] Or emit individual \`PaymentCreated\` events for each item
- [ ] Unit test asserting all payment_ids present in event"

i "batch_payments: duplicate payment IDs within a single batch not validated" "A batch request could include the same payment_id twice. The contract may process both, creating a double-registration.

**Acceptance criteria:**
- [ ] Validate all payment_ids in batch are unique
- [ ] Return \`Error::DuplicatePaymentId\` on duplicate
- [ ] Unit test with duplicate payment_id in batch"

i "rbac_access: role assignment events not emitted — audit trail for permission changes missing" "When roles are assigned or revoked via \`rbac_access\`, no event is emitted. Admin actions are invisible to off-chain monitoring.

**Acceptance criteria:**
- [ ] Emit \`RoleAssigned { account, role, assigned_by, timestamp }\`
- [ ] Emit \`RoleRevoked { account, role, revoked_by, timestamp }\`
- [ ] Backend indexes RBAC events for audit log
- [ ] Unit test for event emission"

i "rbac_access: no role hierarchy — ADMIN and SUPERADMIN have same permissions" "RBAC contract may not enforce a role hierarchy where SUPERADMIN has all ADMIN permissions plus additional ones.

**Acceptance criteria:**
- [ ] Define role hierarchy: VIEWER < OPERATOR < ADMIN < SUPERADMIN
- [ ] \`has_permission(account, permission)\` checks all roles in hierarchy
- [ ] Unit test: ADMIN cannot perform SUPERADMIN-only actions"

i "rbac_access: role check not used by all contracts — payment_escrow bypasses RBAC for some functions" "Some high-privilege functions in payment_escrow may check auth directly instead of delegating to rbac_access, creating inconsistent permission enforcement.

**Acceptance criteria:**
- [ ] Audit all admin functions across all contracts
- [ ] All admin functions must use rbac_access.has_permission()
- [ ] Remove inline auth checks that duplicate RBAC logic
- [ ] Integration test: revoke role → function returns unauthorized"

i "rbac_access: no maximum roles per account — unbounded storage per account" "An account can be assigned an unlimited number of roles, potentially creating a very large storage entry.

**Acceptance criteria:**
- [ ] Cap roles per account at 10
- [ ] Return \`Error::TooManyRoles\` on exceeded
- [ ] Unit test for role cap"

i "admin_timelock: timelock duration not validated — 0-second timelock accepted" "The admin timelock may accept a duration of 0, effectively bypassing the timelock for any operation.

**Acceptance criteria:**
- [ ] Minimum timelock duration: 24 hours (configurable by governance)
- [ ] Return \`Error::TimelockTooShort\` for < minimum
- [ ] Unit test for minimum duration enforcement"

i "admin_timelock: queued operation can be executed by any caller — not restricted to proposer" "If any account can execute a queued timelock operation, an attacker could race to execute an admin operation before the proposer cancels it.

**Acceptance criteria:**
- [ ] Only the original proposer OR a designated executor can execute
- [ ] Admin can assign executors per operation
- [ ] Unit test: non-proposer cannot execute"

i "admin_timelock: no event on operation queue — off-chain monitoring cannot detect pending ops" "When an operation is queued in the timelock, no event is emitted. Monitoring systems cannot alert on pending high-risk operations.

**Acceptance criteria:**
- [ ] Emit \`OperationQueued { op_id, operation, proposer, execute_after }\`
- [ ] Emit \`OperationExecuted { op_id, executed_by }\`
- [ ] Emit \`OperationCancelled { op_id, cancelled_by }\`
- [ ] Unit test for each event"

i "admin_timelock: no cancellation window after timelock expires — operations executable forever" "Once a timelocked operation's window opens, it remains executable indefinitely. If the proposer forgets to execute or cancel, the operation can be executed days later.

**Acceptance criteria:**
- [ ] Add execution deadline: timelock_period + max_execution_window (e.g. 48h)
- [ ] Operation expires and is auto-cancelled after deadline
- [ ] \`Error::OperationExpired\` returned on expired execution attempt"

i "admin_timelock: timelock cannot be used for multi-step operations — single op only" "The timelock queues one operation at a time. Some admin workflows require multiple atomically related operations.

**Acceptance criteria:**
- [ ] Support operation batches: vec of operations with a single timelock
- [ ] All operations in a batch execute atomically
- [ ] Unit test: batch of 3 operations all execute or all fail"

i "multisig_admin: quorum not validated on initialization — 0-of-N threshold accepted" "The multisig admin contract may accept a quorum of 0, allowing any single operation to pass without signatures.

**Acceptance criteria:**
- [ ] Validate quorum >= 1 and quorum <= signers.len()
- [ ] Return \`Error::InvalidQuorum\` on out-of-bounds
- [ ] Unit test for quorum validation edge cases (0, 1, N, N+1)"

i "multisig_admin: signer list not validated for duplicates — same signer counted multiple times" "If the signer list passed to multisig initialization contains duplicates, the same key could sign multiple times and satisfy quorum alone.

**Acceptance criteria:**
- [ ] Validate all signer addresses are unique
- [ ] Return \`Error::DuplicateSigner\` on duplicate
- [ ] Unit test with duplicate signers"

i "multisig_admin: no event on signature collection — cannot monitor progress toward quorum off-chain" "As signers approve an operation, no events are emitted. Off-chain monitoring cannot track approval progress.

**Acceptance criteria:**
- [ ] Emit \`SignatureAdded { op_id, signer, approvals_so_far, quorum_required }\`
- [ ] Emit \`QuorumReached { op_id }\`
- [ ] Unit test for signature collection events"

i "multisig_admin: signer rotation not timelocked — compromised signer can immediately remove others" "Signer rotation (add/remove) is not timelocked. A compromised signer key that meets quorum can immediately remove all other signers and take control.

**Acceptance criteria:**
- [ ] Signer rotation operations must go through admin_timelock
- [ ] Minimum timelock for signer changes: 48h
- [ ] Unit test: signer removal without timelock is rejected"

i "multisig_admin: no maximum number of signers — unbounded storage for signer set" "An unlimited number of signers can be added, potentially creating a very large storage entry and long signature verification loops.

**Acceptance criteria:**
- [ ] Cap signers at 20
- [ ] Return \`Error::TooManySigners\` on exceeded
- [ ] Unit test for signer cap"

i "slippage_protection: slippage check uses spot price — susceptible to flash loan manipulation" "Slippage is validated against the current spot price. A flash loan can manipulate the price within a single transaction to bypass slippage protection.

**Acceptance criteria:**
- [ ] Use TWAP (time-weighted average price) over last N ledgers instead of spot
- [ ] Admin-configurable TWAP window in ledgers
- [ ] Unit test demonstrating spot manipulation bypassed by TWAP"

i "slippage_protection: protection can be disabled by admin — no governance requirement" "An admin can disable slippage protection via a simple admin call. This should require multisig and timelock governance.

**Acceptance criteria:**
- [ ] Disable slippage protection requires multisig_admin quorum
- [ ] Disable operation goes through admin_timelock (24h minimum)
- [ ] Emit \`SlippageProtectionDisabled\` event
- [ ] Unit test for governance requirement"

i "slippage_protection: no event when slippage limit exceeded — silent failure" "When a transaction is rejected due to slippage, no event is emitted. The backend cannot distinguish slippage rejection from other errors.

**Acceptance criteria:**
- [ ] Emit \`SlippageExceeded { payment_id, quoted_rate, actual_rate, max_slippage_bps }\`
- [ ] Backend parses event to trigger user notification
- [ ] Unit test for event on slippage rejection"

i "stellar_confirmations: no replay protection on confirmation submission" "If the same confirmation is submitted twice, it may be processed twice, potentially double-crediting a payment.

**Acceptance criteria:**
- [ ] Use tx_hash as idempotency key in storage
- [ ] Return \`Error::AlreadyConfirmed\` on duplicate
- [ ] Unit test for idempotent replay"

i "stellar_confirmations: confirmation not validated against expected payment amount" "A confirmation submission with an amount different from the payment request is accepted without validation.

**Acceptance criteria:**
- [ ] Validate confirmed_amount is within acceptable_tolerance of requested_amount
- [ ] Return \`Error::AmountMismatch\` on significant discrepancy
- [ ] Admin-configurable tolerance (default 1%)
- [ ] Unit test for tolerance boundary"

i "stellar_confirmations: no timeout for unconfirmed transactions — pending confirmations accumulate" "Transactions submitted for confirmation that are never confirmed remain in pending state indefinitely, consuming storage.

**Acceptance criteria:**
- [ ] Pending confirmations expire after expiry_ledgers
- [ ] Expired confirmations cleaned up by admin/cron
- [ ] Emit \`ConfirmationExpired\` event
- [ ] Unit test for expiry cleanup"

i "stellar_confirmations: confirmation threshold configurable but not validated — 0 confirmations accepted" "The required confirmation count is configurable but may accept 0, allowing instant unconfirmed finalization.

**Acceptance criteria:**
- [ ] Minimum required_confirmations: 1
- [ ] Return \`Error::InvalidConfirmationThreshold\` for 0
- [ ] Unit test for threshold validation"

i "platform_stats: stats not updated atomically — concurrent updates cause race conditions" "Platform-wide stats (total volume, payment count) are updated per transaction without an atomic increment mechanism, potentially causing race conditions under concurrent load.

**Acceptance criteria:**
- [ ] Use Soroban atomic storage primitives for stat updates
- [ ] Or use per-ledger stat deltas merged in a reconciliation pass
- [ ] Unit test for concurrent stat updates"

i "platform_stats: no historical stats — only current totals stored" "Platform stats only store current totals with no historical snapshots. Trend analysis requires the backend to track history externally.

**Acceptance criteria:**
- [ ] Store daily stat snapshots in contract storage (rolling 30 days)
- [ ] \`get_stats_history(days)\` function
- [ ] Admin function to prune old snapshots
- [ ] Unit test for snapshot creation and retrieval"

i "admin_auth: admin key rotation not tested end-to-end" "Admin authentication key rotation is not tested in an end-to-end scenario (old key → rotate → new key works, old key rejected).

**Acceptance criteria:**
- [ ] Test: admin action with old key succeeds
- [ ] Test: rotate key
- [ ] Test: admin action with new key succeeds
- [ ] Test: admin action with old key fails after rotation"

i "admin_auth: no event on admin auth failure — brute-force attempts invisible" "Failed admin authentication attempts do not emit an event. An attacker attempting to brute-force admin keys is completely invisible on-chain.

**Acceptance criteria:**
- [ ] Emit \`AdminAuthFailed { caller, function, timestamp }\` on auth failure
- [ ] Backend indexes auth failure events and alerts on threshold
- [ ] Unit test for failure event emission"

i "admin_auth: admin credentials not stored with key derivation — raw public key comparison" "Admin authentication may compare the caller's public key directly against a stored public key without additional key derivation, making credential rotation complex.

**Acceptance criteria:**
- [ ] Document admin auth mechanism clearly
- [ ] Support multiple admin keys for key rotation without downtime
- [ ] Unit test for multi-admin key management"

i "testing: payment_escrow has no fuzz tests for input validation" "Critical escrow functions (deposit, release, refund, dispute) have unit tests but no fuzz tests covering malformed inputs.

**Acceptance criteria:**
- [ ] Fuzz test: deposit with random amounts, addresses, payment_ids
- [ ] Fuzz test: release with random caller addresses
- [ ] Fuzz test: dispute with oversized reason strings
- [ ] CI runs fuzz tests for 60 seconds per function"

i "testing: no integration tests covering full payment_escrow + merchant_registry flow" "Unit tests cover individual contracts. No integration test covers the full flow: register merchant → create payment → deposit → release.

**Acceptance criteria:**
- [ ] Integration test deploying both contracts to testnet (or simnet)
- [ ] Full flow from registration to settlement
- [ ] Error path: unregistered merchant attempt
- [ ] CI runs integration tests on every PR"

i "testing: fee_calculator not tested with all supported token decimals" "USDC has 7 decimal places on Stellar. Fee calculations may be incorrect for other token decimal configurations.

**Acceptance criteria:**
- [ ] Test fee calculation for 6-decimal tokens
- [ ] Test fee calculation for 7-decimal tokens (USDC)
- [ ] Test fee calculation for 0-decimal tokens (edge case)
- [ ] Assert no precision loss in any configuration"

i "testing: batch_payments instruction usage not benchmarked — may exceed limits at max batch size" "No benchmark exists for batch_payments at maximum batch size to verify it stays within Soroban instruction limits.

**Acceptance criteria:**
- [ ] Benchmark: batch of 20 payments measures instruction usage
- [ ] Assert usage < 80% of Soroban instruction limit (safety margin)
- [ ] Document max safe batch size in README
- [ ] CI benchmark step fails if limit exceeded"

i "testing: slippage_protection not tested with real AMM price calculations" "Slippage protection tests use hardcoded prices. Real AMM price curves are not tested.

**Acceptance criteria:**
- [ ] Test with constant product AMM formula
- [ ] Test with large trades that move price significantly
- [ ] Test with minimal liquidity edge case"

i "testing: settlement_ledger has no test for maximum entries per merchant" "No test verifies behavior when a merchant has accumulated the maximum number of settlement records.

**Acceptance criteria:**
- [ ] Test: settlement at max count returns correct response
- [ ] Test: pagination works correctly at max count
- [ ] Test: archival frees space for new settlements"

i "testing: reconciliation contract not tested with intentionally corrupted state" "Reconciliation tests assume well-formed state. No tests verify behavior when contract state is intentionally corrupted.

**Acceptance criteria:**
- [ ] Test: reconciliation with manually corrupted amount field
- [ ] Test: reconciliation with missing payment entry
- [ ] Test: reconciliation with duplicate payment IDs in ledger"

i "testing: multisig_admin not tested for signer set changes during active operation" "No test verifies behavior when a signer is removed while an operation is waiting for signatures.

**Acceptance criteria:**
- [ ] Test: remove signer whose signature was already counted — does operation still reach quorum?
- [ ] Test: add signer mid-operation — new quorum applies or old?
- [ ] Document and implement consistent behavior"

i "testing: admin_timelock tests do not cover timelock clock manipulation" "No test verifies that the timelock cannot be bypassed by manipulating the ledger timestamp.

**Acceptance criteria:**
- [ ] Test: operation cannot execute before timelock period expires
- [ ] Test: operation executes exactly at timelock expiry
- [ ] Test: operation expires after execution window"

i "testing: WASM size enforcement missing from CI — contracts may exceed 64KB limit silently" "The Makefile has a \`check-wasm-size\` target but it may not run in CI. Contracts exceeding 64KB fail on deployment.

**Acceptance criteria:**
- [ ] CI runs \`make check-wasm-size\` on every push
- [ ] CI fails if any contract WASM exceeds 64KB
- [ ] Alert threshold at 48KB (75%) for early warning"

i "build: no reproducible builds — WASM output not deterministic across machines" "Rust/WASM builds may produce different binary outputs on different machines or CI agents, making build verification impossible.

**Acceptance criteria:**
- [ ] Pin Rust toolchain version in rust-toolchain.toml
- [ ] CI produces the same WASM hash on every run
- [ ] Document verification procedure for reproducible builds"

i "build: wasm-opt optimization level not documented — Oz vs O3 tradeoff unclear" "The Makefile uses \`wasm-opt -Oz\` (size-optimized) but the tradeoff between code size and execution cost is not documented.

**Acceptance criteria:**
- [ ] Benchmark contract execution cost with Oz vs O3
- [ ] Document chosen optimization level and rationale in README
- [ ] Add benchmark results to Makefile output"

i "build: no CI check for unused Rust dependencies in contracts" "Unused dependencies in Cargo.toml increase compile time and may introduce vulnerabilities.

**Acceptance criteria:**
- [ ] Run \`cargo machete\` or \`cargo udeps\` in CI
- [ ] Fail CI on unused dependencies found
- [ ] Clean up all existing unused dependencies"

i "build: Cargo.lock not committed for contracts — reproducible builds impossible" "If \`Cargo.lock\` is not committed for contract crates, \`cargo build\` may resolve different dependency versions on different machines.

**Acceptance criteria:**
- [ ] Commit Cargo.lock for all contract crates
- [ ] CI uses \`--locked\` flag on cargo build and test
- [ ] Document lock file update procedure"

i "security: no key ceremony documented for mainnet admin key generation" "No procedure exists for securely generating and storing the mainnet admin keys used by admin_timelock and multisig_admin.

**Acceptance criteria:**
- [ ] Document key ceremony procedure in docs/security/
- [ ] Keys generated offline on airgapped hardware
- [ ] Multi-party key generation (MPC) or hardware security module required
- [ ] Disaster recovery procedure for lost admin keys"

i "security: emergency_drain recipient not restricted to an admin-approved allowlist" "\`emergency_drain()\` takes a destination address as a parameter. Without an allowlist, a compromised admin key can drain funds to any address.

**Acceptance criteria:**
- [ ] Admin-managed drain_allowlist in contract storage
- [ ] emergency_drain() only sends to allowlisted addresses
- [ ] Allowlist changes require multisig + timelock
- [ ] Unit test for allowlist enforcement"

i "security: contract upgrade does not preserve storage schema — silent data corruption possible" "When \`upgrade()\` replaces the WASM, if the new contract has a different storage schema, existing state may be misread or corrupted.

**Acceptance criteria:**
- [ ] Document storage schema versioning strategy
- [ ] Add storage_version field to contract state
- [ ] New WASM validates storage_version on first function call
- [ ] Upgrade test: deploy v1, write state, upgrade to v2, read state correctly"

i "security: no rate limiting on contract function calls — DoS via repeated failed calls" "Any Stellar account can call contract functions repeatedly, consuming Soroban CPU budget and potentially causing DoS for legitimate users.

**Acceptance criteria:**
- [ ] Track call count per account per ledger in storage
- [ ] Return \`Error::RateLimited\` after threshold (e.g. 100 calls/ledger)
- [ ] Admin-configurable per-function limits
- [ ] Unit test for rate limit enforcement"

i "security: soroban contract IDs hardcoded in backend .env — no on-chain registry for discovery" "Contract IDs are stored in backend environment variables. If a contract is upgraded with a new ID, all backend instances must be manually updated.

**Acceptance criteria:**
- [ ] Deploy a contract registry that maps contract names to current IDs
- [ ] Backend queries registry on startup and caches IDs
- [ ] Contract ID changes update the registry with an event
- [ ] Unit test for registry lookup"

i "security: no on-chain audit log for sensitive admin operations" "Admin operations (drain, upgrade, role changes) are visible in transaction history but not aggregated in a queryable on-chain audit log.

**Acceptance criteria:**
- [ ] Dedicated admin_audit_log contract or storage key
- [ ] All sensitive operations append to audit log with: op_type, actor, timestamp, params
- [ ] \`get_audit_log(limit, cursor)\` function
- [ ] Unit test for audit log append and query"

i "dx: no contract ABIs published — backend must hardcode function signatures" "Contract ABIs are not published to a registry or included in the repo in a machine-readable format. The backend hardcodes function names and argument types.

**Acceptance criteria:**
- [ ] Generate and commit ABI JSON for each contract post-build
- [ ] CI step: build contracts and verify ABI files are up to date
- [ ] Backend uses ABI files to construct contract calls
- [ ] ABI versioned alongside contract"

i "dx: no contract documentation beyond README — function signatures undocumented" "Contract functions have minimal inline documentation. \`///\` doc comments are sparse or missing.

**Acceptance criteria:**
- [ ] All public functions have /// doc comments
- [ ] Parameters documented with valid ranges and meanings
- [ ] Error codes documented with causes
- [ ] cargo doc generates clean HTML docs"

i "dx: no local simnet setup documented — developers must test against testnet" "No documentation exists for running a local Stellar simnet for contract testing without network dependency.

**Acceptance criteria:**
- [ ] Document stellar-cli local network setup
- [ ] docker-compose.dev.yml includes Stellar simnet container
- [ ] README: single command to start local dev environment"

i "dx: Makefile has no help target — available commands undiscoverable" "Running \`make\` without a target may produce an error or run an unexpected default target. Available commands are undiscoverable.

**Acceptance criteria:**
- [ ] Add \`make help\` target listing all commands with descriptions
- [ ] \`help\` is the default target
- [ ] CI step verifies \`make help\` runs without error"

i "dx: no code coverage tracking for contracts — coverage silently drops between PRs" "No code coverage tool (tarpaulin or similar) is integrated into CI for contracts.

**Acceptance criteria:**
- [ ] Add \`cargo tarpaulin\` to CI pipeline
- [ ] Set coverage threshold: 70% minimum per contract
- [ ] Coverage report uploaded to Codecov (codecov.yml already exists — wire it up)
- [ ] CI fails if coverage drops below threshold"

i "dx: test fixtures not shared across contracts — each contract reimplements the same helpers" "Each contract's test module reimplements common test helpers (deploy contract, fund account, create payment). This duplication makes tests longer and harder to maintain.

**Acceptance criteria:**
- [ ] Extract common test helpers to a \`test-utils\` crate in the workspace
- [ ] All contract test modules import from test-utils
- [ ] Remove duplicated helper code from individual contracts"

i "dx: no changelog or semantic versioning for contracts — breaking changes untracked" "Contract versions are not tracked. Breaking changes to function signatures or storage layouts are undetectable from the changelog.

**Acceptance criteria:**
- [ ] CHANGELOG.md per contract with semver entries
- [ ] Breaking changes documented with migration notes
- [ ] Cargo.toml version field updated per semver rules
- [ ] CI check: version must be bumped if public function signatures change"

i "dx: grafana/loki configuration not documented for local developer use" "Grafana and Loki configs exist under grafana/ but their purpose and how to run them locally are undocumented.

**Acceptance criteria:**
- [ ] README section: 'Local Observability Stack'
- [ ] docker-compose includes Grafana + Loki + Promtail
- [ ] Preconfigured dashboards for: payment volume, error rate, contract invocations
- [ ] One-command startup: \`make monitoring\`"

i "dx: monitoring/ directory contents undocumented — uptime setup unclear" "The monitoring/ directory contains status page and uptime configuration but no README explaining how to deploy or use it.

**Acceptance criteria:**
- [ ] monitoring/README.md with setup instructions
- [ ] Environment variables documented
- [ ] Integration with Better Uptime or UptimeRobot explained"

i "observability: no Prometheus metrics exported from contract backend service" "The backend service in dupdapp_stellar/backend/ (if present) may not export Prometheus metrics for contract invocation latency and error rates.

**Acceptance criteria:**
- [ ] Counter: \`contract_invocations_total\` labeled by contract and function
- [ ] Histogram: \`contract_invocation_duration_seconds\`
- [ ] Counter: \`contract_errors_total\` labeled by error type
- [ ] Metrics exposed at /metrics"

i "observability: no alerting on Soroban event indexer falling behind" "If the Soroban event indexer (in backend) falls significantly behind the current ledger, payments and settlements could be delayed without any alert.

**Acceptance criteria:**
- [ ] Alert if indexed_ledger < current_ledger - 1000
- [ ] Prometheus metric: \`soroban_indexer_lag_ledgers\`
- [ ] Dashboard panel for indexer lag
- [ ] Pagerduty/Slack alert on lag > threshold"

i "observability: contract transaction failure rate not tracked — silent degradation" "If contract invocations start failing at a high rate (e.g. fee issues, congestion), there is no alert. The platform degrades silently.

**Acceptance criteria:**
- [ ] Counter: \`contract_tx_failures_total\` labeled by error type
- [ ] Alert if failure rate > 5% over 5 minutes
- [ ] Dashboard showing failure rate over time"

i "observability: no on-chain event for contract deployment — audit trail incomplete" "When a contract is deployed or upgraded, no structured event is emitted that the monitoring system can capture.

**Acceptance criteria:**
- [ ] Deploy script emits a structured log / event with: contract_name, wasm_hash, deployer, timestamp
- [ ] Monitoring system captures deployment events
- [ ] Alert sent to admin on any mainnet deployment"

i "ci: CI pipeline does not run contract tests against testnet — only simnet/unit tests" "CI runs cargo test locally but doesn't deploy and test against Stellar testnet. Contract interactions that work locally may fail on testnet.

**Acceptance criteria:**
- [ ] Add testnet integration test CI job (runs on merge to main only)
- [ ] Deploy all contracts to testnet
- [ ] Run integration test suite against deployed contracts
- [ ] Tear down test contracts after CI run"

i "ci: deploy workflow has no dry-run step — production deploys untested before execution" "The deploy.yml workflow deploys directly without a dry-run or simulation step to catch issues before mainnet execution.

**Acceptance criteria:**
- [ ] Add \`stellar-cli contract simulate\` step before deploy
- [ ] Fail CI if simulation produces errors
- [ ] Document dry-run procedure in deploy.yml comments"

i "ci: no contract size check in PR CI — WASM may exceed limit only discovered on deploy" "WASM size is checked via \`make check-wasm-size\` but this may not run on every PR. Developers discover the size violation only on deploy.

**Acceptance criteria:**
- [ ] Run WASM size check on every PR CI run
- [ ] Add per-contract size budget to Makefile
- [ ] Fail PR if any contract exceeds its budget"

i "ci: cargo test runs with default features — no test run with all features enabled" "Some contract functionality may be behind feature flags. CI only runs tests with default features, leaving feature-gated code untested.

**Acceptance criteria:**
- [ ] Add \`cargo test --all-features\` CI step
- [ ] Verify all feature combinations compile and test correctly
- [ ] Document feature flags in README"

i "ci: code coverage not enforced per contract — global coverage hides poorly tested contracts" "Even if global coverage is tracked, a well-tested contract can mask a 0% covered contract.

**Acceptance criteria:**
- [ ] Run tarpaulin per contract, not just workspace-wide
- [ ] Per-contract coverage threshold in CI config
- [ ] Coverage report shows per-contract breakdown"

i "ci: no security audit of Rust dependencies — vulnerable crates accepted silently" "No \`cargo audit\` step runs in CI. Vulnerable Rust dependencies are accepted without detection.

**Acceptance criteria:**
- [ ] Run \`cargo audit\` in CI on every push
- [ ] Fail CI on high/critical severity advisories
- [ ] Weekly scheduled audit run even without code changes"

i "ci: deploy workflow missing rollback procedure — failed deploy leaves contracts in broken state" "If the deploy workflow fails mid-way (e.g. some contracts deployed, others not), there is no documented rollback procedure.

**Acceptance criteria:**
- [ ] Document rollback procedure in deploy.yml comments
- [ ] Keep previous WASM hashes in a deploy manifest
- [ ] Admin runbook for emergency rollback
- [ ] Test rollback procedure in staging before mainnet"

i "docs: no documented upgrade procedure for payment_escrow — teams don't know how to safely upgrade" "docs/escrow_upgrade_procedure.md exists but may be incomplete. Developers attempting an upgrade may miss critical steps.

**Acceptance criteria:**
- [ ] Step-by-step upgrade checklist with pre/post verification
- [ ] State migration notes for schema changes
- [ ] Test upgrade on testnet before mainnet
- [ ] Sign-off checklist requiring multiple team members"

i "docs: security docs in docs/security/ are sparse — threat model undocumented" "docs/security/ directory exists but may contain only placeholder files. No formal threat model documents the attack surface.

**Acceptance criteria:**
- [ ] Threat model document: attackers, assets, attack vectors, mitigations
- [ ] Security assumptions documented (trusted admin keys, oracle, etc.)
- [ ] Known limitations and accepted risks documented
- [ ] Reviewed by external auditor"

i "docs: no documented disaster recovery procedure for lost admin keys" "If the mainnet admin keys are lost or compromised, there is no documented procedure to recover contract control.

**Acceptance criteria:**
- [ ] docs/security/key-recovery.md
- [ ] Social recovery or multisig fallback procedure
- [ ] Contact procedure for Stellar Foundation assistance
- [ ] Tested in a drill on testnet"

i "docs: contract function reference not generated from source — docs drift from implementation" "Contract documentation is written manually in Markdown. As functions change, the docs drift from actual behavior.

**Acceptance criteria:**
- [ ] Generate function reference from \`///\` doc comments using cargo doc
- [ ] CI publishes docs to GitHub Pages on merge to main
- [ ] CI fails if doc generation produces warnings"

i "docs: no documented procedure for adding a new contract to the workspace" "No guide exists for adding a new Soroban contract to the workspace (Cargo.toml, Makefile targets, CI steps).

**Acceptance criteria:**
- [ ] CONTRIBUTING.md section: 'Adding a new contract'
- [ ] Cargo workspace template for new contracts
- [ ] CI automatically picks up new contracts for build, test, and size check"

echo ""
echo "=== Stellar: 125 issues created ==="
