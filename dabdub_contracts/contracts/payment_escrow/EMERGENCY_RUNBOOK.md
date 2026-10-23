# Payment Escrow Emergency Drain Runbook

## Purpose

Use `emergency_drain` only during a confirmed critical security incident that risks escrowed USDC.

## Preconditions

- Incident commander has declared emergency mode.
- At least 2 authorized emergency signers (from the configured 3) are available.
- The treasury address is verified as the platform multisig wallet.
- Cooldown window from the previous emergency drain has elapsed.

## Execution Steps

1. Confirm current escrow contract USDC balance is non-zero.
2. Prepare a transaction calling:
   - `emergency_drain(caller, signer_one, signer_two)`
3. Ensure both `signer_one` and `signer_two` provide signatures.
4. Submit and confirm transaction success.

## Expected Contract Behavior

- Contract enforces:
  - signer_one and signer_two are distinct
  - both are members of the configured emergency signer set
  - cooldown is not active
- Entire USDC balance held by the escrow contract is transferred atomically to treasury.
- `EmergencyDrain` event (`ESCROW`, `emergency_drain`) is emitted with:
  - `amount`
  - `caller`

## Post-Execution Checks

- Escrow contract USDC balance is `0`.
- Treasury USDC balance increased by the drained amount.
- Event stream contains the emergency drain event for audit trail.

## Cooldown and Repeat Protection

- A successful drain records the ledger sequence.
- Any subsequent drain attempts before `emergency_cooldown_ledgers` elapses must fail.


## Rolling back a bad release

Follow the [mainnet deployment and incident runbook](../../../docs/deployment.md)
for containment, compatible escrow code replacement, and migration of contracts
without an upgrade path. The current escrow ABI requires `schedule_upgrade`,
waiting for the recorded apply ledger, and `apply_upgrade`; it has no immediate
`upgrade` entrypoint. `stellar contract deploy` creates a new instance and does
not roll back the live contract or its storage.

There is no on-chain pause function. A `multisig_admin` proposal is an approval
signal, not enforcement against escrow; off-chain operators must separately stop
intake/submission and verify containment. Direct on-chain calls remain possible.
`emergency_drain` is distinct from both pause and code recovery: it transfers the
configured USDC balance to the emergency treasury, requires caller plus both
emergency signer authorizations, and does not drain XLM or reconcile liabilities.

## Stuck payment recovery (non-functional token contract)

`release`, `release_partial`, `expire`, `refund` and `resolve_dispute` all pay out through
the single internal `transfer_from_contract` helper, which calls the configured token's
`transfer`. If that token contract stops working (paused/frozen by its issuer, SAC reverting,
contract account deauthorized, etc.) every settlement call for payments in that asset fails
atomically — the payment record is **not** modified, so nothing is lost, it is just stuck.

Operator procedure:

1. **Confirm the cause.** Simulate the failing settlement call and check the token contract
   directly (e.g. `balance` / a small `transfer` simulation). Rule out payment-state errors
   (`Payment not releasable`, expiry, etc.) first.
2. **Wait if the outage is temporary.** Payments stay in their current status indefinitely.
   Once the token is functional again, retry the original settlement call. Note that
   `release` cannot succeed after `expiry`; the customer-side `expire`/`refund` path remains
   available, and `resolve_dispute` has no expiry check for already-disputed payments.
3. **Pause new deposits in the affected asset** off-chain (backend/UI) so no new funds get stuck.
4. **If the token is permanently broken**, the only on-chain path is `emergency_drain`
   (2-of-3 emergency signers, see above) to move remaining funds to the emergency treasury
   once the token allows it, followed by off-chain reconciliation per payment
   (`get_payment` / `get_balance`) and manual reimbursement to customers/merchants.
   If the token never allows outbound transfers again, the funds are unrecoverable by any
   contract — document the affected payment IDs and escalate.
5. **Record** the affected payment IDs, token address, and resolution in the incident log.
