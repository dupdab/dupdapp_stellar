# batch_payments

## Purpose

`batch_payments` lets a caller submit a single transaction that pays many
recipients at once. Instead of N separate transfers (N ledger entries, N auth
checks, N events), the caller supplies a batch of `(recipient, amount)` pairs
and the contract executes them atomically: either every payment in the batch
succeeds or the whole call reverts.

This is intended for payroll, airdrops, affiliate payouts, and any flow where a
single operator distributes funds to a known list of addresses.

## Storage

| Key | Type | Description |
| --- | --- | --- |
| `Admin` | `Address` | Address allowed to configure the contract (see Admin operations). |
| `Token` | `Address` | The token contract used for all payments in a batch. |
| `BatchCount` | `u32` | Monotonic counter used to derive unique batch ids. |
| `Batch(id)` | `BatchRecord` | Per-batch record: payer, total amount, recipient count, ledger sequence. |

Storage is instance-scoped; entries are written only when a batch is executed
and are never mutated afterwards.

## Events

| Event | Topics | Data | Emitted when |
| --- | --- | --- | --- |
| `batch_executed` | `(symbol_short!("batch"), payer)` | `(batch_id, recipient_count, total_amount)` | A batch completes successfully. |
| `batch_failed` | `(symbol_short!("batch_fail"), payer)` | `(batch_id, reason_code)` | A batch is rejected before any transfer is attempted. |

Events are emitted once per batch, not once per recipient, to keep the event
stream compact for large batches.

## Admin operations

- `initialize(admin, token)` — one-time setup; sets `Admin` and `Token`.
- `set_token(new_token)` — admin-only; rotates the payment token. Existing
  batch records are unaffected.
- `pause()` / `unpause()` — admin-only; while paused, `execute_batch` reverts
  with a paused error and no transfers occur.

All admin entry points require `admin.require_auth()`.

## Known limitations

- **Single token per batch.** A batch cannot mix assets; callers needing
  multiple tokens must submit one batch per token.
- **No partial success.** A single failing recipient (e.g. insufficient balance
  or a rejecting token) reverts the entire batch. Callers should pre-validate
  balances off-chain or split large batches.
- **Batch size is bounded** by the Soroban per-transaction resource limits;
  very large recipient lists must be chunked across multiple transactions.
- **No refund path.** Once a batch executes, funds are transferred; there is no
  admin clawback. Recovery of misdirected funds depends on the recipient.
- **Pause is global.** Pausing blocks all payers, not a single caller.

## Cross-references

- [`admin_auth`](../admin_auth/README.md) — shared admin/role primitives used
  by the pause and token-rotation entry points.
- [`admin_timelock`](../admin_timelock/README.md) — if admin actions on this
  contract are routed through the timelock, see that contract for the delay
  and cancellation semantics.
- The configured `Token` contract (external SEP-41 token) — all transfers are
  delegated to it; its own pause/allowlist rules apply on top of this
  contract's pause flag.
