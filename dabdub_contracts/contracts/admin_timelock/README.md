# admin_timelock

## Purpose

`admin_timelock` schedules privileged administrative operations so they cannot be
applied instantly. A caller proposes an operation, it becomes executable only
after a configured delay has elapsed, and it can be cancelled before execution.
This gives users and other stakeholders a window to react to sensitive changes
(e.g. parameter updates, upgrades, or role changes) before they take effect.

## Storage

| Key | Type | Description |
| --- | --- | --- |
| `Admin` | `Address` | Current admin authorised to schedule/cancel/execute operations. |
| `Delay` | `u64` | Minimum number of ledgers that must pass between scheduling and execution. |
| `MinDelay` | `u64` | Governance-configurable minimum allowed delay (defaults to 24 hours). |
| `Pending` | `Map<Bytes, ScheduledOp>` | Scheduled operations keyed by an operation id, including the earliest executable ledger and the proposer. |
| `Executors` | `Map<Bytes, Address>` | Optional per-operation executor assigned by the admin. |

Storage entries are written on `schedule` and removed on `execute` or `cancel`.

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

- `scheduled` — emitted when an operation is queued; includes the operation id and the earliest executable ledger.
- `executed` — emitted when a queued operation is applied after the delay.
- `cancelled` — emitted when a queued operation is removed before execution.

## Admin operations

- `initialize(admin, delay)` — set the initial admin and delay. Callable once. Rejects delays below the minimum.
- `schedule(op_id, op)` — admin-only; queue an operation for later execution. Records the proposer.
- `set_executor(op_id, executor)` — admin-only; assign an executor allowed to execute the queued operation.
- `execute(op_id)` — proposer- or executor-only; apply a queued operation once the delay has elapsed.
- `cancel(op_id)` — admin-only; remove a queued operation before it executes.
- `set_delay(delay)` — admin-only; update the delay for future schedules. Rejects delays below the minimum.
- `set_min_delay(min_delay)` — governance-only; update the minimum allowed delay (never below 24 hours).

All mutating entry points require the stored admin's authorisation, except
`execute`, which requires the proposer or the designated executor.

## Known limitations

- The delay is enforced in ledgers, not wall-clock time; actual latency depends on ledger cadence.
- Only one pending operation per `op_id`; re-scheduling the same id overwrites the previous entry.
- Changing the delay does not retroactively affect already-scheduled operations.
- There is no built-in batching: each operation must be scheduled and executed individually.

## Cross-references

- [`admin_auth`](../admin_auth/README.md) — authorisation primitives used to gate admin-only entry points.
- [`access_control`](../access_control/README.md) — role checks that scheduled operations may target.
