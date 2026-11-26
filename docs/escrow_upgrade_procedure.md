# Soroban Contract Upgrade Procedure

This document outlines the upgrade procedure for all contracts in the DupDub Stellar Contracts workspace. It covers dependency ordering, timelock/multisig requirements, rollback procedures, and end-to-end testnet validation steps.

## Overview

There are **16 contract crates** in the workspace. The canonical set listed in the README consists of 14 contracts; `admin_auth` and `platform_stats` are present in the workspace but not listed in the README table. This document treats **all 16** contracts for completeness.

The authoritative procedure is [Mainnet deployment and incident runbook](deployment.md#escrow-compatible-code-replacement-at-the-same-id).

The current escrow ABI uses `schedule_upgrade(caller, new_wasm_hash)`,
`get_pending_upgrade()`, `cancel_upgrade(caller)` and `apply_upgrade(caller)`.
It has no immediate `upgrade` entrypoint. The default timelock is 17,280 ledgers.
Verify the live executable/ABI and required admin authorization before acting.

Upload the reviewed compatible WASM, schedule its hash, verify the pending hash
and apply ledger, wait for that ledger, then apply with admin authorization.
Verify the executable hash and payment/storage invariants afterwards.
`get_version` counts upgrades and is not the code release identity.

`apply_upgrade` replaces the contract code in place; it does not rerun the
constructor or clear persistent storage. Existing `Payment` entries remain
available when the new code preserves their storage key and value schema.
Validate an in-flight payment with `get_payment` and `get_balance` after
upgrading, then confirm the expected settlement action (release or dispute)
still succeeds. Keep the payment expiry and dispute window open long enough for
the upgrade timelock to elapse.

Code can be replaced with an earlier compatible WASM through an available
upgrade path, but storage migrations and past transfers are not reversed.
Do not use `stellar contract deploy` for an in-place rollback: it creates a new
instance with separate storage. See the linked runbook for commands, compatibility
checks, cancellation, forward-fix alternatives and emergency containment.

| Contract | Crate | Has Self-Upgrade | Upgrade Mechanism |
| --- | --- | --- | --- |
| `admin_auth` | `admin-auth` | No | Redeploy new instance (no in-place upgrade function) |
| `admin_timelock` | `admin-timelock` | No | Redeploy new instance (attestation/signaling only; enforces timelock internally) |
| `batch_payments` | `batch-payments` | No | Redeploy new instance |
| `fee_calculator` | `fee-calculator` | No | Redeploy new instance |
| `fee_distributor` | `fee_distributor` | No | Redeploy new instance |
| `liquidity_router` | `liquidity-router` | No | Redeploy new instance |
| `merchant_registry` | `merchant-registry` | No | Redeploy new instance (two-step admin handover available) |
| `multisig_admin` | `multisig-admin` | No | Redeploy new instance (attestation/signaling only; provides quorum) |
| `payment_escrow` | `payment-escrow` | **Yes** | Timelocked in-place upgrade via `schedule_upgrade` / `apply_upgrade` |
| `payment_request` | `payment-request` | No | Redeploy new instance (one-time `initialize`) |
| `platform_stats` | `platform-stats` | No | Redeploy new instance |
| `rbac_access` | `rbac-access` | No | Redeploy new instance |
| `reconciliation` | `reconciliation` | No | Redeploy new instance |
| `settlement_ledger` | `settlement_ledger` | No | Redeploy new instance |
| `slippage_protection` | `slippage-protection` | No | Redeploy new instance |
| `stellar_confirmations` | `stellar-confirmations` | No | Redeploy new instance |

> **Note:** Only `payment_escrow` implements Soroban's in-place WASM upgrade (`env.deployer().update_current_contract_wasm`). All other contracts do **not** have self-upgrade functions; upgrading them means deploying a new contract instance with the new WASM and updating references/callers to the new contract ID.


## Dependency Graph

Cross-contract dependencies (contracts that call or reference other contracts):

```text
payment_escrow ──► merchant_registry  (RegistryContract; calls is_approved)
batch_payments ──► merchant_registry  (RegistryContract; calls is_approved)
stellar_confirmations ──► payment_escrow (PaymentEscrowContract; calls get_payment)
```

All other contracts have no cross-contract invocations and depend only on external token/AMM/pool contracts configured by address (e.g., `fee_distributor` uses `UsdcToken`, `liquidity_router` calls AMM pools via `AmmClient`).

### Recommended Upgrade Order (Bottom-Up)

Upgrade dependencies first to avoid breaking callers. Order by dependency levels:

| Phase | Contracts | Rationale |
| --- | --- | --- |
| **0** | `admin_timelock`, `multisig_admin`, `rbac_access`, `admin_auth`, `fee_calculator`, `fee_distributor`, `liquidity_router`, `payment_request`, `platform_stats`, `reconciliation`, `settlement_ledger`, `slippage_protection` | No upstream dependencies within the 16-contract set. Can be upgraded in any order among themselves. |
| **1** | `merchant_registry` | Required by `payment_escrow` and `batch_payments`. Must be upgraded before Phase 2 if its interface changes in a breaking way. |
| **2** | `payment_escrow`, `batch_payments` | Depend on `merchant_registry`. If `payment_escrow` interface changes, upgrade before Phase 3. |
| **3** | `stellar_confirmations` | Depends on `payment_escrow`. Upgrade last among dependent contracts. |

> If a contract has no cross-contract dependencies, the order is flexible. Always verify call sites and stored contract addresses after upgrading dependencies.


## Timelock Requirements

| Contract | Timelock Required | Details |
| --- | --- | --- |
| `payment_escrow` | **Yes** | Built-in timelock: `schedule_upgrade(caller, new_wasm_hash)` stores pending upgrade with `apply_ledger = current_ledger + UpgradeDelayLedgers` (default **17,280 ledgers** ≈ 24 hours at 5s/ledger). `apply_upgrade(caller)` requires the delay to have elapsed; `cancel_upgrade(caller)` cancels before application. `get_pending_upgrade()` reports pending state. |
| `admin_timelock` | **Yes** | Enforces minimum timelock: `schedule_change(caller, param, value, delay_ledgers)` requires `delay_ledgers >= MIN_DELAY_LEDGERS` (**17,280 ledgers** ≈ 24 hours). `apply_change` only after delay; `cancel_change` before execution. This contract is **attestation/signaling only** (emits events) — it does not mutate target contracts directly. |
| All others | **No (built-in)** | No self-upgrade timelock mechanism. If using `admin_timelock` to gate changes to these contracts, timelock is enforced by `admin_timelock` composition, not by the contract itself. |

## Multisig Quorum Requirements

| Contract | Multisig Required | Details |
| --- | --- | --- |
| `payment_escrow` | **Recommended (quorum)** | Admin is a single address. **Recommended deployment:** set `Admin` to a `multisig_admin` (2-of-3) instance so that `schedule_upgrade`/`apply_upgrade`/`cancel_upgrade`/admin config changes require a 2-of-3 quorum. `multisig_admin` uses `THRESHOLD = 2` of 3 admins, 24h proposal expiry, auto-execution on threshold (attestation/signaling only). |
| `admin_timelock` | **Recommended (quorum)** | Admin is a single address. **Recommended deployment:** set `admin` to a `multisig_admin` (2-of-3) instance so `schedule_change`/`apply_change`/`cancel_change` require quorum (as documented in `admin_timelock`). |
| `multisig_admin` | **Yes (by design)** | Provides 2-of-3 quorum mechanism (`THRESHOLD=2`, exactly 3 admins). Not upgraded via quorum of itself in the usual sense — see rollback notes. |
| `rbac_access` | **No** | Role-based; SuperAdmin operations require `require_role(SuperAdmin)`. No built-in multisig. |
| `admin_auth` | **No** | SuperAdmin-gated operations. No built-in multisig. |
| `merchant_registry` | **No** | Admin-gated with two-step handover (`propose_admin`/`accept_admin`). No built-in multisig/quorum. |
| All others | **No** | Admin-gated (single admin or `AllowedCaller`/`SettlementCaller`) unless deployed behind multisig/timelock governance. |

> **Critical:** A compromised single key can defeat timelock/multisig if governance contracts are deployed with an EOA as admin. Always compose `multisig_admin` + `admin_timelock` as recommended.


## Upgrade Procedures

### 1) PaymentEscrow (Self-Upgrading, Timelocked)

**Prerequisites:**
- Soroban CLI installed and configured for target network.
- Admin credentials (address stored in `PaymentEscrow` `DataKey::Admin`).
- If admin is a `multisig_admin`, quorum approval required for `schedule_upgrade`/`apply_upgrade`.
- New contract WASM compiled for `wasm32v1-none`.
- Sufficient XLM for fees.

**Step-by-Step:**

1. **Build the new WASM**
   ```bash
   cd dabdub_contracts
   cargo build --target wasm32v1-none --release
   # Optionally optimize
   wasm-opt -Oz --output target/wasm32v1-none/release/payment_escrow.wasm target/wasm32v1-none/release/payment_escrow.wasm
   ```

2. **Upload the new WASM**
   ```bash
   stellar contract install \
     --wasm target/wasm32v1-none/release/payment_escrow.wasm \
     --source <ADMIN_SECRET> \
     --network <NETWORK>
   ```
   Record the returned **WASM hash** (e.g., `ae34...`).

3. **Schedule the upgrade (timelock starts)**
   ```bash
   stellar contract invoke \
     --id <PAYMENT_ESCROW_CONTRACT_ID> \
     --source <ADMIN_SECRET> \
     --network <NETWORK> \
     -- \
     schedule_upgrade \
     --caller <ADMIN_ADDRESS> \
     --new_wasm_hash <NEW_WASM_HASH>
   ```
   Verify pending upgrade:
   ```bash
   stellar contract invoke \
     --id <PAYMENT_ESCROW_CONTRACT_ID> \
     --network <NETWORK> \
     -- \
     get_pending_upgrade
   ```
   Note the `apply_ledger` (current ledger + ~17,280). **Wait for the timelock to elapse.**

4. **Apply the upgrade**
   ```bash
   stellar contract invoke \
     --id <PAYMENT_ESCROW_CONTRACT_ID> \
     --source <ADMIN_SECRET> \
     --network <NETWORK> \
     -- \
     apply_upgrade \
     --caller <ADMIN_ADDRESS>
   ```

5. **Verify the upgrade**
   ```bash
   stellar contract invoke \
     --id <PAYMENT_ESCROW_CONTRACT_ID> \
     --network <NETWORK> \
     -- \
     get_version
   ```
   Version should increment by 1.

**Important Considerations:**
- **Storage compatibility:** New WASM must be compatible with existing storage layout (`PaymentEscrow` struct, `DataKey` variants). Adding fields requires migration logic in `apply_upgrade`/upgrade path before/at upgrade.
- **Admin restriction:** Only `DataKey::Admin` can schedule/apply/cancel. If multisig, all calls require quorum.
- **State preservation:** All instance/persistent storage preserved on in-place upgrade.
- **Emergency signers:** `emergency_drain`, `set_registry(None)` have additional safeguards (cooldown, multi-signer authorization).

### 2) AdminTimelock (Attestation/Signaling Contract)

`admin_timelock` has no self-upgrade. Changes are scheduled via `schedule_change` with timelock; `apply_change` emits attestation events (external relayer applies to targets). Upgrade procedure: deploy new instance, migrate admin/state as needed, update governance composition.

```bash
# Schedule a change (example)
stellar contract invoke --id <ADMIN_TIMELOCK_ID> --source <ADMIN_SECRET> --network <NETWORK> -- \
  schedule_change --caller <ADMIN_ADDRESS> --param <PARAM> --value <VALUE> --delay_ledgers 17280

# Apply after delay
stellar contract invoke --id <ADMIN_TIMELOCK_ID> --source <ADMIN_SECRET> --network <NETWORK> -- \
  apply_change --caller <ADMIN_ADDRESS> --change_id <CHANGE_ID>
```

### 3) MultisigAdmin (Quorum/Signaling Contract)

`multisig_admin` has no self-upgrade. It is attestation/signaling only (stores proposals/approvals, auto-executes on 2-of-3 threshold by flipping flags/emitting events; does not dispatch cross-contract calls). Upgrade = deploy new instance, migrate admin set/threshold if needed, update references to the multisig address.

```bash
# Propose
stellar contract invoke --id <MULTISIG_ID> --source <PROPOSER_SECRET> --network <NETWORK> -- \
  propose --caller <PROPOSER_ADDRESS> --operation "<OP>" --args <BYTES>

# Approve (triggers auto-execute at threshold)
stellar contract invoke --id <MULTISIG_ID> --source <APPROVER_SECRET> --network <NETWORK> -- \
  approve --caller <APPROVER_ADDRESS> --proposal_id <ID>
```

### 4) All Other Contracts (Redeploy New Instance)

For contracts with **no self-upgrade** (`admin_auth`, `batch_payments`, `fee_calculator`, `fee_distributor`, `liquidity_router`, `merchant_registry`, `payment_request`, `platform_stats`, `rbac_access`, `reconciliation`, `settlement_ledger`, `slippage_protection`, `stellar_confirmations`):

1. **Build and install new WASM**
   ```bash
   cd dabdub_contracts
   cargo build --target wasm32v1-none --release
   stellar contract install --wasm target/wasm32v1-none/release/<contract>.wasm --source <ADMIN_SECRET> --network <NETWORK>
   # record WASM hash
   ```

2. **Deploy new contract instance** (constructor/initialize as appropriate)
   ```bash
   stellar contract deploy \
     --wasm-hash <NEW_WASM_HASH> \
     --source <DEPLOYER_SECRET> \
     --network <NETWORK> \
     -- \
     __constructor <CONSTRUCTOR_ARGS>
   # or call initialize() for contracts using initialize pattern
   ```
   Record the **new contract ID**.

3. **Migrate configuration/state** (if required)
   - Copy necessary configuration (admin, addresses, parameters) to the new instance.
   - For contracts with persistent storage holding critical state (e.g., `settlement_ledger` records, `reconciliation` batches, `merchant_registry` merchants, `rbac_access` roles), state does **not** automatically transfer. Plan and execute migration (off-chain scripts or dedicated migration contracts) carefully.
   - Update two-step admin handovers if in progress (`merchant_registry` has `pending_admin`).

4. **Update references**
   - Update all contracts/configs that store the old contract address (e.g., `payment_escrow` stores `RegistryContract` → update via `set_registry`; `batch_payments` stores `RegistryContract`; `stellar_confirmations` stores `PaymentEscrowContract`; `fee_calculator` stores `SettlementCaller`; `fee_distributor` stores `AllowedCaller`; `liquidity_router` allowlists).
   - Update off-chain services (backend, indexers) to use new contract IDs.

5. **Verify and cutover**
   - Test the new instance on testnet.
   - Cut over traffic/callers to new instance.
   - Retire old instance only after verification and rollback window.


## Rollback Notes Per Contract

| Contract | Rollback Strategy | Notes |
| --- | --- | --- |
| `payment_escrow` | **Cancel or revert WASM.** If not yet applied: `cancel_upgrade(caller)` removes pending upgrade. If already applied: schedule/apply previous WASM hash with timelock (version increments again). State preserved. Emergency procedures: `emergency_drain` (requires 2 of 3 emergency signers, 24h cooldown) for USDC drain in emergencies. | Timelock prevents immediate rollback; plan accordingly. |
| `admin_timelock` | Redeploy old WASM to new instance, or cancel pending changes via `cancel_change` before apply. | Attestation-only; no direct state mutation to targets. Pending changes can be pruned via `prune_change`. |
| `multisig_admin` | Redeploy old WASM to new instance. Migrate admin set/threshold/proposals if needed. | Attestation-only (signaling). Proposals expire after 24h; can prune expired via `prune_expired_proposal`. |
| `merchant_registry` | Redeploy old instance; restore state if possible, or revert callers to old instance. If admin transfer in progress, `pending_admin` remains until accepted or replaced. | Two-step handover allows cancelling/re-proposing. |
| `payment_request` | Redeploy old instance; state not auto-migrated (initialized once). | Requests stored persistently; migration required. |
| `batch_payments` | Redeploy old instance; revert registry/limits config. | Payments stored persistently keyed by payment ID. |
| `fee_calculator` | Redeploy old instance; restore tiers/settlement caller/min fee. | Tiers stored in instance storage; merchant volumes persistent. |
| `fee_distributor` | Redeploy old instance; restore admin, validated recipient list, USDC token, and allowed caller. | The recipient-list constructor is incompatible with the legacy treasury/LP schema; plan a migration and redeploy. See [deployment configuration](deployment_configuration.md). |
| `liquidity_router` | Redeploy old instance; restore admin/allowlist/depth threshold. | Allowlist stored in instance storage. |
| `platform_stats` | Redeploy old instance; counters not auto-migrated. | Rolling buckets in persistent storage with TTL. |
| `reconciliation` | Redeploy old instance; batches stored persistently; ensure batch history preserved if needed. | Multiple batches retained (by ID). Use `get_batch`/`get_latest_stored_batch`. |
| `settlement_ledger` | Redeploy old instance; **settlement records are immutable and not auto-migrated**. | Critical: plan migration carefully. Records append-only; void markers stored separately. |
| `slippage_protection` | Redeploy old instance; restore global/pair slippage limits. | Simple config. |
| `stellar_confirmations` | Redeploy old instance; restore config and payment_escrow link. Confirmation state not auto-migrated. | Per-payment confirmation state persistent. |
| `admin_auth` | Redeploy old instance; restore SuperAdmin/admin index/admin records. | Admin entries stored persistently. |
| `rbac_access` | Redeploy old instance; restore roles/SuperAdminCount. | Roles stored in persistent storage (moved from instance in fix for #1026). |

**General rollback rules:**
- For contracts without self-upgrade: keep old contract instance alive until rollback window closes and all callers migrated back safely.
- For `payment_escrow`: use timelock to your advantage (schedule revert, wait, apply) or cancel if pending.
- Always verify admin/authority continuity after rollback.
- Preserve event history for auditability.


## Test the Procedure End-to-End on Testnet

> **Note:** Execution requires `stellar-cli` (or Soroban CLI), testnet access, funded accounts, and appropriate permissions. This is a procedure to follow; commands should be adapted to your environment.

### Pre-Test Checklist
- [ ] All contracts compile: `cd dabdub_contracts && cargo build --target wasm32v1-none --release`
- [ ] Tests pass: `cd dabdub_contracts && cargo test`
- [ ] WASM sizes within limit: `make check-wasm-size`
- [ ] Testnet accounts funded (admin, deployer, emergency signers, multisig participants)
- [ ] Contract IDs recorded for all deployed contracts
- [ ] Current versions/state snapshots taken

### Test Plan

#### Test 1: PaymentEscrow Timelocked Upgrade (Critical Path)
1. Build and install new `payment_escrow` WASM → get hash.
2. Call `get_version()` and `get_pending_upgrade()` (expect None).
3. `schedule_upgrade(caller, new_wasm_hash)` → verify event `upgrade_scheduled`, `get_pending_upgrade()` returns hash + apply_ledger.
4. Attempt `apply_upgrade()` before timelock elapses → **expect panic** ("Upgrade timelock has not elapsed").
5. Wait for timelock to elapse (or advance ledger in local/test environment).
6. `apply_upgrade(caller)` → verify event/state, `get_version()` incremented, `get_pending_upgrade()` returns None.
7. **Rollback test:** Schedule previous WASM hash, cancel with `cancel_upgrade()` before apply → verify pending cleared.

#### Test 2: AdminTimelock Change Scheduling
1. `schedule_change(caller, "test_param", "test_value", 17280)` → get change_id.
2. `get_change(change_id)` shows status `Pending`.
3. Attempt `apply_change()` before delay → expect panic.
4. Wait for delay, `apply_change()` → status becomes `Applied`, event emitted.
5. Test `cancel_change()` on another pending change.
6. Test `prune_change()` on applied/cancelled change.

#### Test 3: MultisigAdmin Quorum
1. Create proposal with 1 approval (proposer) → not executed (threshold 2).
2. Second admin approves → auto-executes, event `proposal_executed` emitted.
3. Test `prune_expired_proposal()` on expired unexecuted proposal.
4. Test threshold invariants (cannot go below 2).

#### Test 4: Dependency Upgrade (MerchantRegistry → PaymentEscrow)
1. Upgrade `merchant_registry` (redeploy new instance, migrate config/admin).
2. Update `payment_escrow` registry: `set_registry(caller, Some(new_registry_address), None)`.
3. Verify `deposit()` still enforces merchant approval via new registry.
4. Test `set_registry(..., None, Some((s1,s2)))` requires two emergency signers.

#### Test 5: Cross-Contract Dependency (StellarConfirmations → PaymentEscrow)
1. Ensure `payment_escrow` updated and verified.
2. Update `stellar_confirmations`: `set_payment_escrow_contract(caller, Some(escrow_address))`.
3. Test `confirm_payment()` cross-checks amount/merchant against escrow.
4. Verify mismatch causes panic as expected.

#### Test 6: Redeploy-Only Contracts
For a sample contract (e.g., `fee_calculator`):
1. Deploy new instance with constructor args.
2. Update all callers to new address.
3. Verify functionality.
4. Verify old instance remains accessible for rollback if needed.

### Validation Checklist
- [ ] All upgraded contracts pass smoke tests
- [ ] Cross-contract calls work with updated addresses
- [ ] Admin/multisig permissions enforced correctly
- [ ] Timelock delays enforced
- [ ] Events emitted correctly
- [ ] No storage corruption
- [ ] Rollback procedures tested

### Notes
- Use `stellar contract invoke` or `soroban contract invoke` depending on CLI version.
- Replace placeholders (`<...>`) with actual values.
- For testnet, be mindful of ledger timing for timelock tests.
- Document all contract IDs, WASM hashes, and transaction hashes during testing.

## Migration Logic
If the upgrade requires data migration (e.g., transforming old storage formats to new ones), the `upgrade` function in `lib.rs` should be modified to include the migration logic *before* calling `update_current_contract_wasm`.

## Upgrade Sequence for All 14 Contracts

All 14 contracts in `dabdub_contracts/contracts/` must be upgraded in dependency order. A contract may only be upgraded after every contract it depends on has been upgraded and verified. Upgrading a dependent before its dependency can leave the dependent pointing at an incompatible interface or storage layout.

### Dependency Graph

```
admin_timelock  (root — no contract dependencies)
      |
      +--> escrow
      |       |
      |       +--> payment_escrow
      |       +--> refund
      |
      +--> registry
      |       |
      |       +--> reputation
      |       +--> dispute
      |               |
      |               +--> arbitration
      |
      +--> treasury
              |
              +--> fee_manager
                      |
                      +--> payout
                              |
                              +--> settlement
```

### Ordered Upgrade Sequence

Upgrade strictly top-to-bottom. Verify each contract (see Step 4) before proceeding to the next.

| # | Contract | Depends On | Timelock Required | Multisig Quorum |
|---|----------|------------|-------------------|-----------------|
| 1 | `admin_timelock` | — | No (is the timelock) | Yes (2-of-3) |
| 2 | `escrow` | `admin_timelock` | Yes | Yes (2-of-3) |
| 3 | `payment_escrow` | `escrow` | Yes | Yes (2-of-3) |
| 4 | `refund` | `escrow` | Yes | No |
| 5 | `registry` | `admin_timelock` | Yes | Yes (2-of-3) |
| 6 | `reputation` | `registry` | No | No |
| 7 | `dispute` | `registry` | Yes | Yes (2-of-3) |
| 8 | `arbitration` | `dispute` | Yes | Yes (2-of-3) |
| 9 | `treasury` | `admin_timelock` | Yes | Yes (2-of-3) |
| 10 | `fee_manager` | `treasury` | Yes | No |
| 11 | `payout` | `fee_manager` | Yes | No |
| 12 | `settlement` | `payout` | Yes | Yes (2-of-3) |
| 13 | `access_control` | `admin_timelock` | Yes | Yes (2-of-3) |
| 14 | `governance` | `admin_timelock`, `access_control` | Yes | Yes (3-of-5) |

### Timelock Requirements

Contracts marked **Timelock Required** must schedule the upgrade through `admin_timelock` and wait out the configured delay before the `upgrade` invocation is allowed to execute. The flow is:

1. `admin_timelock::schedule_upgrade(contract_id, new_wasm_hash)` — starts the delay clock.
2. Wait for the timelock delay to elapse.
3. `admin_timelock::execute_upgrade(contract_id)` — performs the upgrade.

`admin_timelock` itself is upgraded directly (it cannot gate its own upgrade) but still requires multisig quorum.

### Multisig Quorum Requirements

Contracts marked **Yes** in the Multisig Quorum column require the admin address to be a multisig account and the upgrade transaction to be signed by the stated threshold. Contracts marked **No** may be upgraded by a single admin key, but should still be reviewed by a second signer before execution.

## Rollback Notes per Contract

Soroban does not support in-place rollback of a WASM upgrade. To roll back, re-install the previous WASM hash and invoke `upgrade` again with that hash. Keep the previous WASM hash recorded for every contract before upgrading.

| Contract | Rollback Notes |
|----------|----------------|
| `admin_timelock` | Re-upgrade to previous hash directly (no timelock). Verify pending scheduled upgrades are cleared. |
| `escrow` | Re-upgrade via timelock. Confirm no in-flight escrows were created against the new storage layout. |
| `payment_escrow` | Re-upgrade via timelock. Reconcile any payments created post-upgrade; storage additions are not auto-reverted. |
| `refund` | Re-upgrade via timelock. Verify refund records written under the new layout are readable by the old code. |
| `registry` | Re-upgrade via timelock. Registry entries added post-upgrade may need manual cleanup. |
| `reputation` | Re-upgrade directly. Reputation scores written post-upgrade should be snapshotted first. |
| `dispute` | Re-upgrade via timelock. Open disputes created post-upgrade must be resolved or migrated before rollback. |
| `arbitration` | Re-upgrade via timelock. Arbitration rulings issued post-upgrade must be preserved. |
| `treasury` | Re-upgrade via timelock. Reconcile balances moved post-upgrade before rolling back. |
| `fee_manager` | Re-upgrade via timelock. Fee config changes made post-upgrade must be re-applied. |
| `payout` | Re-upgrade via timelock. Pending payouts created post-upgrade must be drained or migrated. |
| `settlement` | Re-upgrade via timelock. Settlements finalized post-upgrade are immutable; verify before rollback. |
| `access_control` | Re-upgrade via timelock. Role grants made post-upgrade must be re-applied after rollback. |
| `governance` | Re-upgrade via timelock with 3-of-5 quorum. Active proposals created post-upgrade must be cancelled or migrated. |

## End-to-End Testnet Validation

Before upgrading mainnet, run the full sequence on testnet:

1. Deploy all 14 contracts at the current version.
2. Install the new WASM for each contract.
3. Execute the ordered upgrade sequence above, verifying `get_version` after each step.
4. Exercise a representative flow (create escrow → dispute → arbitrate → settle) to confirm cross-contract calls still work.
5. Perform a rollback on at least one contract (e.g. `refund`) and confirm the previous version is restored.

### Notes
- Use `stellar contract invoke` or `soroban contract invoke` depending on CLI version.
- Replace placeholders (`<...>`) with actual values.
- For testnet, be mindful of ledger timing for timelock tests.
- Document all contract IDs, WASM hashes, and transaction hashes during testing.
