# admin_auth

Role-based authorization registry used by the other contracts in this workspace to gate privileged operations.

## Purpose

`admin_auth` maintains the set of addresses that are allowed to perform administrative actions (e.g. pausing, upgrading, changing parameters) in dependent contracts. Dependent contracts query this contract instead of hard-coding admin addresses, so admin membership can be rotated in one place.

## Storage

- Admin set: the collection of addresses currently holding admin rights.
- Owner / super-admin: the address permitted to add or remove admins.
- Admin audit log: an append-only, ordered log of sensitive admin operations, stored under a dedicated `admin_audit_log` storage key.

(Exact storage keys are defined in `src/lib.rs`; this section is a summary, not a normative spec.)

## Events

- Admin added — emitted when a new admin address is granted rights.
- Admin removed — emitted when an admin address is revoked.

Consumers should index these events to keep off-chain views of admin membership in sync.

## Admin operations

- `add_admin` — owner-only; grants admin rights to an address.
- `remove_admin` — owner-only; revokes admin rights from an address.
- `is_admin` — read-only check used by dependent contracts.

## Admin audit log

Every sensitive admin operation (drain, upgrade, role changes such as `add_admin` / `remove_admin`) appends an entry to the on-chain audit log. Each entry records:

- `op_type` — the kind of operation performed (e.g. `drain`, `upgrade`, `add_admin`, `remove_admin`).
- `actor` — the address that authorized the operation.
- `timestamp` — ledger timestamp at the time the entry was appended.
- `params` — operation-specific parameters (e.g. target address, amount, new wasm hash).

Entries are append-only and ordered by insertion, so the log can be replayed to reconstruct the history of privileged actions.

### Querying

- `get_audit_log(limit, cursor)` — returns a page of audit entries plus a `next_cursor`. Pass `cursor = None` to start from the beginning; pass the returned `next_cursor` to fetch the following page. When `next_cursor` is `None`, the end of the log has been reached.

## Known limitations

- Admin membership is stored on-chain only; there is no built-in expiry or timelock. Pair with `admin_timelock` if delayed execution is required.
- Revoking an admin does not retroactively cancel actions already authorized by that admin.
- No pagination helper for enumerating admins; large admin sets may be expensive to read off-chain.
- The audit log grows unbounded; consumers should page through it with `get_audit_log` rather than reading it in full.

## Cross-references

- `admin_timelock` — schedules and delays privileged operations; typically combined with `admin_auth` for defense in depth.
- Any contract that performs privileged operations should consult `admin_auth::is_admin` rather than maintaining its own admin list.
