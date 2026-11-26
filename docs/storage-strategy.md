# Soroban Storage Strategy

This document describes how the Dabdub contracts use Soroban storage, why each
storage class was chosen, and how TTL (time-to-live) extension is handled. It
also estimates the storage cost of a single payment under the current Stellar
fee schedule.

## Storage classes in Soroban

Soroban exposes three storage classes, each with different lifetime and cost
characteristics:

| Class | Scope | Lifetime | Eviction | Cost |
|-------|-------|----------|----------|------|
| `instance` | Per-contract-instance | Tied to the contract instance entry; lives as long as the instance is not archived | Archived with the instance | Cheapest per read/write; shared TTL across all instance keys |
| `persistent` | Per-key | Explicit TTL, extended by the contract | Never auto-evicted while TTL is maintained; can be archived if TTL lapses | Most expensive; pays rent for the full TTL window |
| `temporary` | Per-key | Explicit TTL, short-lived | Evicted automatically when TTL expires; no restore | Cheapest per entry, but data is lost on expiry |

Key rules of thumb:

- **Instance** storage is for small, contract-wide configuration that every
  invocation needs (admin, token addresses, counters). It is read/written on
  every call, so it should stay small.
- **Persistent** storage is for data that must survive indefinitely and be
  recoverable (balances, per-user records, escrow state). The contract is
  responsible for extending TTL before it lapses.
- **Temporary** storage is for data that is only meaningful for a bounded
  window (nonces, short-lived approvals, rate-limit buckets). It is allowed to
  expire and is never restored.

## Per-contract storage usage

### `admin_auth`

- **Instance**: `Admin`, `Threshold`, `SignerSet` (the multisig configuration).
  These are read on every authorization check and are small and fixed-size, so
  instance storage avoids per-call rent.
- **Persistent**: none.
- **Temporary**: pending proposal approvals keyed by proposal id. Approvals are
  only valid until the proposal is executed or expires, so temporary storage is
  appropriate and avoids paying persistent rent for short-lived data.

**Justification**: the admin configuration is contract-wide and hot; approvals
are bounded-lifetime and disposable.

### `admin_timelock`

- **Instance**: `Admin`, `MinDelay`, `PendingAdmin`.
- **Persistent**: queued operations keyed by operation id, including the
  earliest execution ledger. Queued operations must survive until executed or
  cancelled, which can be many ledgers later, so they cannot be temporary.
- **Temporary**: none.

**Justification**: queued operations are durable state that must be restorable;
configuration is hot and small.

### `batch_payments`

- **Instance**: `Admin`, `Token`, `FeeCollector` addresses and the batch counter.
- **Persistent**: per-recipient payment records and batch metadata that must be
  auditable after the fact.
- **Temporary**: per-batch idempotency keys / nonces used to reject replays.
  These only need to live for the replay window.

**Justification**: payment records are durable audit data; replay guards are
bounded-lifetime.

### `fee_calculator`

- **Instance**: fee parameters (basis points, min/max fee, token decimals).
  These are read on every fee computation and are small.
- **Persistent**: none.
- **Temporary**: none.

**Justification**: the calculator is stateless apart from its configuration,
which is hot and contract-wide.

### `fee_distributor`

- **Instance**: `Admin`, `Token`, recipient list length.
- **Persistent**: per-recipient accrued balances and share configuration that
  must survive until claimed.
- **Temporary**: none.

**Justification**: accrued balances are user funds and must never be evicted;
configuration is hot.

### `liquidity_router`

- **Instance**: `Admin`, registered pool addresses, router configuration.
- **Persistent**: per-pool routing state and any escrowed amounts.
- **Temporary**: quote/route cache entries keyed by (pool, amount) that are only
  valid for a short window.

**Justification**: routing state and escrow are durable; quotes are disposable.

## TTL extension strategy

Soroban entries have a TTL measured in ledgers. The contracts follow a
consistent policy:

1. **Instance TTL** is extended on every state-changing entry point via
   `env.storage().instance().extend_ttl(threshold, extend_to)`. The threshold is
   set so that the instance is refreshed well before it would be archived.
2. **Persistent TTL** is extended whenever a persistent key is written or read
   on a mutating path, using `extend_ttl` with a threshold and target chosen per
   contract (see below). Read-only paths do not extend TTL to avoid charging
   callers for rent they did not request.
3. **Temporary TTL** is set at write time to the minimum window that covers the
   feature's validity (e.g. the replay window or proposal lifetime). It is never
   extended, because expiry is the intended cleanup mechanism.

Per-contract TTL targets (in ledgers, assuming ~5s per ledger):

| Contract | Instance threshold / extend_to | Persistent threshold / extend_to | Temporary TTL |
|----------|-------------------------------|----------------------------------|---------------|
| `admin_auth` | 1 day / 7 days | n/a | proposal lifetime |
| `admin_timelock` | 1 day / 7 days | 7 days / 30 days | n/a |
| `batch_payments` | 1 day / 7 days | 7 days / 30 days | replay window |
| `fee_calculator` | 1 day / 7 days | n/a | n/a |
| `fee_distributor` | 1 day / 7 days | 7 days / 30 days | n/a |
| `liquidity_router` | 1 day / 7 days | 7 days / 30 days | quote window |

Operators should monitor TTLs and call the contracts' maintenance entry points
(or re-invoke a mutating path) before persistent entries approach their
threshold.

## Storage cost per payment

This estimate uses the Stellar fee schedule as of the Soroban mainnet launch
(refer to `https://developers.stellar.org/docs/learn/fees` for the current
values). The relevant inputs are:

- **Base inclusion fee**: 100 stroops per operation (1 stroop = 1e-7 XLM).
- **Resource fee**: charged for CPU instructions, ledger reads/writes, and
  rent. Rent is charged per ledger-entry-byte per ledger, currently on the order
  of `1e-5` stroops per byte per ledger for persistent entries.
- **Ledger entry size**: a payment record (recipient `Address` + `i128` amount +
  metadata) is roughly 120 bytes serialized.

For a single payment through `batch_payments`:

1. **Instance read/write**: ~2 reads + 1 write of the small instance entry
   (~200 bytes). Negligible rent; dominated by the base resource fee.
2. **Persistent write**: one payment record (~120 bytes) with a 30-day TTL
   (~518,400 ledgers). Rent ≈ `120 bytes × 518,400 ledgers × 1e-5 stroops`
   ≈ **622 stroops** (~0.0000622 XLM) for the initial TTL window.
3. **Temporary write**: one replay key (~40 bytes) for a short window; rent is
   well under 100 stroops.
4. **Base + resource fee**: the inclusion fee plus CPU/memory resource fee is
   typically a few hundred stroops for a simple transfer.

**Total estimated storage cost per payment: ~1,000–1,500 stroops
(~0.0001–0.00015 XLM)**, dominated by the persistent payment record's rent over
its TTL window. This is an order-of-magnitude estimate; actual cost depends on
serialized size, TTL chosen, and the live fee schedule. Re-run the estimate
whenever the fee schedule or record layout changes.

## Assumptions and caveats

- Ledger close time is assumed to be ~5 seconds.
- Rent rates and resource fees are taken from the published Stellar fee
  schedule and may change via protocol upgrades.
- Sizes are approximate serialized sizes; use `env.storage()` instrumentation
  or the Soroban CLI to measure exact footprints before relying on these
  numbers for budgeting.
