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

Draining is for funds at risk. If the incident is a bad code release rather than
a live exploit, roll the WASM back instead.

**Never roll back with `stellar contract deploy`.** It installs the WASM and
creates a brand-new contract with a new ID and empty storage. The live contract
— holding every open escrow, the admin address and the registry pointer — is
untouched and still runs the bad code, while every downstream service continues
to point at it. That is not a rollback; it is deploying an unrelated, unpopulated
copy.

`payment_escrow` implements `upgrade`, which calls
`update_current_contract_wasm` on the existing contract, so its code can be
swapped in place with all storage preserved:

1. Install the previous release WASM to obtain its hash:

   ```bash
   stellar contract install \
     --wasm <prev-release>/payment_escrow.wasm \
     --source <DEPLOYER> \
     --network mainnet
   ```

2. Invoke `upgrade` on the live contract with that hash:

   ```bash
   stellar contract invoke \
     --id <LIVE_PAYMENT_ESCROW_ID> \
     --source <DEPLOYER> \
     --network mainnet \
     -- upgrade \
     --caller <ADMIN_ADDRESS> \
     --new_wasm_hash <PREV_WASM_HASH>
   ```

   `upgrade` is admin-only; a multisig admin needs the required signatures.

3. Confirm with `get_version`. It increments on every upgrade, a downgrade
   included — it counts upgrades and does not name the code version, so verify
   against the on-chain WASM hash rather than trusting the number.

The full procedure, including storage-compatibility constraints and migration
logic, is in [`docs/escrow_upgrade_procedure.md`](../../../docs/escrow_upgrade_procedure.md).

### The other contracts have no in-place path

`payment_escrow` is the only contract in this workspace with an `upgrade`
entrypoint. The other 13 cannot have their code replaced. Rolling one back means
deploying the previous WASM as a new contract and repointing every reference to
the old ID — `payment_escrow`'s registry pointer via `set_registry`, the NestJS
backend's configured addresses, and any other contract holding the ID. Storage
does not carry over, so this is a migration and needs planning as one.

The same guidance is printed by the `Rollback procedure reminder` step at the end
of the mainnet job in [`.github/workflows/deploy.yml`](../../../.github/workflows/deploy.yml);
keep the two in step if either changes.

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
