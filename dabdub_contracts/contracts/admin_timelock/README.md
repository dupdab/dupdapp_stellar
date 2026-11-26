# admin_timelock

## Purpose

`admin_timelock` schedules privileged administrative operations so they cannot be
applied instantly. A caller proposes an operation, it becomes executable only
after a configured delay has elapsed, and it can be cancelled before execution.
This gives users and other stakeholders a window to react to sensitive changes
(e.g. parameter updates, upgrades, or role changes) before they take effect.

To prevent stale operations from being applied long after they were proposed,
each operation also has a bounded execution window: it must be executed within
`max_execution_window` ledgers after the timelock delay elapses. Operations that
are not executed in time expire and are auto-cancelled.

Some admin workflows require several atomically related operations. A single
timelock entry may therefore hold a *batch* of operations that are executed
together, all-or-nothing.

## Storage

| Key | Type | Description |
| --- | --- | --- |
| `Admin` | `Address` | Current admin authorised to schedule/cancel/execute operations. |
| `Delay` | `u64` | Minimum number of ledgers that must pass between scheduling and execution. |
| `MinDelay` | `u64` | Governance-configurable minimum allowed delay (defaults to 24 hours). |
| `Pending` | `Map<Bytes, ScheduledOp>` | Scheduled operations keyed by an operation id, including the earliest executable ledger and the execution deadline. |
| `Executors` | `Map<Bytes, Address>` | Optional per-operation executor assigned by the admin. |

Storage entries are written on `schedule` and removed on `execute`, `cancel`, or
automatic expiry.

## Constants

- `MAX_EXECUTION_WINDOW` — maximum number of ledgers an operation may remain
executable after its timelock delay elapses (e.g. 48h worth of ledgers).

## Delay validation

A timelock delay of `0` would effectively bypass the timelock, so every delay is
validated against a minimum before it is stored or used:

- The minimum defaults to **24 hours** and is configurable by governance via
  `set_min_delay(min_delay)`.
- `initialize`, `set_delay`, and `set_min_delay` return `Error::TimelockTooShort`
  when the supplied duration is below the configured minimum.
- `set_min_delay` itself cannot lower the minimum below the 24-hour floor.

## Execution authorization

A queued operation can only be executed by an authorised caller, preventing an
attacker from racing to apply an admin operation before the proposer cancels it:

- The **original proposer** (the account that called `schedule`) may always execute.
- The admin may designate an **executor** for a specific operation via
  `set_executor(op_id, executor)`; that executor may then execute the operation.
- Any other caller is rejected with `Error::Unauthorized`.

## Events

Off-chain monitoring can subscribe to these events to detect and alert on
pending high-risk operations before they take effect:

- `OperationQueued { op_id, operation, proposer, execute_after }` — emitted when
  an operation is queued; includes the operation id, the operation payload, the
  proposer, and the earliest executable ledger.
- `OperationExecuted { op_id, executed_by }` — emitted when a queued operation is
  applied after the delay; includes the operation id and the caller that executed it.
- `OperationCancelled { op_id, cancelled_by }` — emitted when a queued operation is
  removed before execution; includes the operation id and the caller that cancelled it.
- `OperationExpired { op_id }` — emitted when a queued operation is auto-cancelled
  because its execution deadline passed.

## Admin operations

- `initialize(admin, delay)` — set the initial admin and delay. Callable once. Rejects delays below the minimum.
- `schedule(op_id, op)` — admin-only; queue an operation for later execution. Records the proposer and emits `OperationQueued`. The execution deadline is computed as `current_ledger + delay + MAX_EXECUTION_WINDOW`.
- `schedule_batch(op_id, ops)` — admin-only; queue a batch (`Vec`) of operations under a single timelock entry. The batch shares one earliest executable ledger and one execution deadline.
- `set_executor(op_id, executor)` — admin-only; assign an executor allowed to execute the queued operation.
- `execute(op_id)` — proposer- or executor-only; apply a queued operation once the delay has elapsed and before the deadline. Returns `Error::OperationExpired` (and auto-cancels the operation) if the deadline has passed. Emits `OperationExecuted`.
- `execute_batch(op_id)` — proposer- or executor-only; apply every operation in a queued batch atomically. If any operation fails, the whole batch reverts and nothing is applied.
- `cancel(op_id)` — admin-only; remove a queued operation (or batch) before it executes. Emits `OperationCancelled`.
- `set_delay(delay)` — admin-only; update the delay for future schedules. Rejects delays below the minimum.
- `set_min_delay(min_delay)` — governance-only; update the minimum allowed delay (never below 24 hours).

All mutating entry points require the stored admin's authorisation, except
`execute`, which requires the proposer or the designated executor.

## Admin key ceremony

The `Admin` address is a high-value mainnet key. It must never be generated on a
networked machine. The full procedure for generating, storing, and recovering
this key (and the `multisig_admin` keys) is documented in
[`docs/security/key-ceremony.md`](../../../docs/security/key-ceremony.md).

In short:

- Keys are generated **offline on airgapped hardware** dedicated to the ceremony.
- Generation requires **multi-party computation (MPC)** or a **hardware security
  module (HSM)**; no single operator ever holds the full private key.
- A documented **disaster recovery** procedure covers loss of admin keys.

## Batching semantics

- A batch is stored as a `Vec` of operations under a single `op_id`.
- All operations in a batch execute atomically: either every operation is
  applied or the entire batch reverts and the entry remains pending.
- A batch shares a single timelock delay and execution window; it cannot be
  partially executed or partially cancelled.

## Known limitations

- The delay and execution window are enforced in ledgers, not wall-clock time; actual latency depends on ledger cadence.
- Only one pending entry per `op_id`; re-scheduling the same id overwrites the previous entry.
- Changing the delay does not retroactively affect already-scheduled operations.

## Cross-references

- [`admin_auth`](../admin_auth/README.md) — authorisation primitives used to gate admin-only entry points.
- [`access_control`](../access_control/README.md) — role checks that scheduled operations may target.
- [`docs/security/key-ceremony.md`](../../../docs/security/key-ceremony.md) — mainnet admin key ceremony and disaster recovery.
