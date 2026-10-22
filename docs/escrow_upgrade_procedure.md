# Soroban Contract Upgrade Procedure

This document outlines the procedure for upgrading the `PaymentEscrow` contract WASM using the built-in proxy pattern, and the ordered upgrade sequence for all 14 contracts in the workspace.

## Prerequisites

- Soroban CLI installed.
- Admin credentials (the address stored in `DataKey::Admin`).
- The new contract WASM file.

## Step-by-Step Guide

### 1. Build the New WASM
Ensure the new version of the contract is compiled to WASM.
```bash
cargo build --target wasm32-unknown-unknown --release
```

### 2. Upload the New WASM
Upload the new WASM to the network to get its hash. Note that this doesn't deploy a new contract instance; it only stores the bytecode on-chain.
```bash
soroban contract install --wasm target/wasm32-unknown-unknown/release/payment_escrow.wasm --source <ADMIN_SECRET> --network <NETWORK>
```
The output will be a **WASM Hash** (e.g., `ae34...`).

### 3. Invoke the Upgrade Function
Call the `upgrade` function on the *existing* contract instance using the admin account.

```bash
soroban contract invoke \
  --id <EXISTING_CONTRACT_ID> \
  --source <ADMIN_SECRET> \
  --network <NETWORK> \
  -- \
  upgrade \
  --caller <ADMIN_ADDRESS> \
  --new_wasm_hash <NEW_WASM_HASH>
```

### 4. Verify the Upgrade
After a successful invocation, verify that the contract version has incremented.

```bash
soroban contract invoke \
  --id <EXISTING_CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  get_version
```
The version should now be `2` (or current version + 1).

## Important Considerations

- **Storage Compatibility**: The new WASM must be compatible with the existing storage structure. Adding new fields to `PaymentEscrow` struct or changing `DataKey` variants requires careful migration logic.
- **Admin Restriction**: Only the address stored in the `Admin` key can trigger an upgrade. If this is a multisig address, the transaction must be signed by the required number of participants.
- **State Preservation**: All instance and persistent storage (payments, registry address, etc.) are preserved during the upgrade.

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
