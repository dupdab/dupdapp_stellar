# Emergency Runbook: admin_timelock

## Overview
The `admin_timelock` contract gates all privileged parameter changes across the Stellar platform.

## Contract Upgrade Sequence (all 14 contracts)

Upgrades must follow the dependency graph below. A contract may only be upgraded after every
contract it depends on has been upgraded and verified on-chain.

```
admin_timelock (root — no deps)
  └─> escrow
        ├─> escrow_factory
        ├─> escrow_milestone
        └─> escrow_dispute
              └─> escrow_arbitration
                    └─> escrow_refund
  └─> payment_splitter
        └─> payment_router
              └─> payment_fee
  └─> reputation
        └─> reputation_badge
  └─> governance
        └─> governance_voting
```

Ordered upgrade sequence (execute top-to-bottom):

| # | Contract | Depends on | Timelock required | Multisig quorum |
|---|----------|------------|-------------------|-----------------|
| 1 | admin_timelock | — | No (is the timelock) | Yes (2-of-3) |
| 2 | escrow | admin_timelock | Yes | Yes (2-of-3) |
| 3 | escrow_factory | escrow | Yes | No |
| 4 | escrow_milestone | escrow | Yes | No |
| 5 | escrow_dispute | escrow | Yes | Yes (2-of-3) |
| 6 | escrow_arbitration | escrow_dispute | Yes | Yes (2-of-3) |
| 7 | escrow_refund | escrow_arbitration | Yes | No |
| 8 | payment_splitter | admin_timelock | Yes | No |
| 9 | payment_router | payment_splitter | Yes | No |
| 10 | payment_fee | payment_router | Yes | Yes (2-of-3) |
| 11 | reputation | admin_timelock | Yes | No |
| 12 | reputation_badge | reputation | Yes | No |
| 13 | governance | admin_timelock | Yes | Yes (2-of-3) |
| 14 | governance_voting | governance | Yes | Yes (2-of-3) |

### Timelock
Every contract except `admin_timelock` itself must be scheduled through `admin_timelock`
(`schedule_change` → wait `execute_after` → `execute_change`). Never bypass the timelock for a
production upgrade.

### Multisig quorum
Contracts marked "Yes (2-of-3)" require signatures from 2 of the 3 admin keys before the
upgrade transaction is submitted. The remaining contracts may be upgraded by a single admin
key, but still go through the timelock.

### Procedure
1. Upgrade `admin_timelock` first (multisig, no timelock).
2. For each subsequent contract in table order: schedule via timelock, wait, execute, then
   verify the new WASM hash and a smoke-test invocation before moving to the next row.
3. Do not upgrade a dependent contract until its dependency is verified.

## Rollback Notes per Contract

| Contract | Rollback action |
|----------|-----------------|
| admin_timelock | Re-deploy previous WASM via multisig; timelock cannot self-gate its own rollback. |
| escrow | Restore previous WASM; existing escrows keep state (storage layout unchanged). |
| escrow_factory | Restore previous WASM; factory registry is append-only, safe to revert. |
| escrow_milestone | Restore previous WASM; verify milestone counters after revert. |
| escrow_dispute | Restore previous WASM; open disputes must be re-verified. |
| escrow_arbitration | Restore previous WASM only after `escrow_dispute` is stable. |
| escrow_refund | Restore previous WASM; pending refunds re-queued. |
| payment_splitter | Restore previous WASM; split configs preserved. |
| payment_router | Restore previous WASM only after `payment_splitter` is stable. |
| payment_fee | Restore previous WASM; fee tables preserved. |
| reputation | Restore previous WASM; scores are additive, safe to revert. |
| reputation_badge | Restore previous WASM; badges already minted remain. |
| governance | Restore previous WASM; in-flight proposals must be re-checked. |
| governance_voting | Restore previous WASM only after `governance` is stable. |

Roll back in reverse dependency order (14 → 1) to avoid dependents pointing at a reverted
dependency.

## Incident: Malicious/Erroneous Scheduled Change

### Symptoms
- A scheduled change has been proposed that would lock out users or drain funds
- The change is scheduled but not yet executed

### Response Steps

1. **Verify the pending change**
```bash
soroban contract invoke \
  --id ADMIN_TIMELOCK_CONTRACT_ID \
  -- get_pending_changes
```

2. **Assess urgency** - check time until `execute_after`

3. **Initiate cancellation** - if admin key available:
```bash
soroban contract invoke \
  --id ADMIN_TIMELOCK_CONTRACT_ID \
  --source ADMIN_KEYPAIR \
  -- cancel_change \
  --change_id CHANGE_ID
```

4. **Verify cancellation** - re-query to confirm

### Prevention
- Keep admin key in secure hardware wallet
- Require multisig for admin operations where possible
- Monitor all scheduled changes via event logs
- Use short `ledgers_to_lock` values (1-2 hours)
