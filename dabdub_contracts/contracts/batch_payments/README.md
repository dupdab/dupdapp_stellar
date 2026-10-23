# batch_payments

## Purpose

`batch_payments` lets an authorized merchant create multiple payment records
in one invocation. The merchant supplies a batch of payment inputs and the
contract validates all entries before storing any, so an invalid item reverts
the entire call.

This supports upstream payroll, airdrop, and affiliate-payout flows where the
payment records are created in batches and settled downstream.

## Storage

| Key | Type | Description |
| --- | --- | --- |
| `Admin` | `Address` | Address allowed to configure the contract (see Admin operations). |
| `MinAmount` / `MaxAmount` | `i128` | Inclusive allowed amount range for each payment. |
| `MaxBatchSize` | `u32` | Configured upper bound; at most 100. |
| `Counter` | `u64` | Monotonic counter used when deriving payment ids. |
| `Version` | `u32` | Contract ABI version. |
| `RegistryContract` | `Address` | Optional merchant registry used to gate batch creation. |
| `Payment(id)` | `PaymentRecord` | Persistent payment record containing the merchant, amount, memo, and optional customer. |

Configuration is instance-scoped. Each payment record is stored persistently
when a batch is created and is not mutated afterwards.

## Events

| Event | Topics | Data | Emitted when |
| --- | --- | --- | --- |
| `PaymentCreated` | `("PaymentCreated",)` | `(payment_id, merchant, amount, memo, customer)` | Each payment record is created. |

One event is emitted per payment record.

## Admin operations

- `init(admin, min_amount, max_amount)` — one-time setup of the administrator
  and per-payment amount limits.
- `set_limits(min_amount, max_amount)` — admin-only amount-limit update.
- `set_max_batch_size(new_max)` — admin-only limit update, capped at 100.
- `set_registry(registry)` — admin-only; configure or clear the optional
  merchant registry.

All admin entry points require `admin.require_auth()`.

## Batch size and instruction benchmark

The default and hard maximum batch size is 100 items. The admin can lower the
configured limit but cannot raise it above this ceiling. The limit is based on
the Soroban test-host CPU instruction benchmark in
`src/test.rs::benchmark_batch_instruction_cost_per_item`, which measures the
contract call separately from setup at 20 and 100 items. In the current test
host run, 20 items used 849,890 instructions (42,494 per item), and 100 items
used 6,853,384 (68,533 per item). The 100-item test also asserts that the call
remains below the 100,000,000-instruction transaction budget; this run used
about 6.9% of that budget.

The loop writes one persistent payment record and emits one event per payment.
Keeping the hard limit at 100 leaves transaction resources for authorization,
input encoding, and surrounding transaction work; callers should still
simulate against the target network and split batches if future contract logic
materially increases cost. The benchmark is a reproducible test-host
measurement, not a substitute for network simulation or a guarantee for
arbitrary future code changes.

Run the measurement with:

```sh
cd dabdub_contracts
cargo test -p batch-payments benchmark_batch_instruction_cost_per_item -- --nocapture
```

## Known limitations

- **No token transfer.** The contract records payment instructions; settlement
  is handled by the downstream payment flow.
- **No partial success.** A single invalid item reverts the entire batch.
- **Batch size is bounded** by the contract's 100-item ceiling; larger payment
  lists must be chunked across multiple transactions.
- **No batch deletion.** Created payment records are immutable and have no
  admin clawback or refund path.

## Cross-references

- [`merchant_registry`](../merchant_registry/README.md) — optional merchant
  approval gate.
