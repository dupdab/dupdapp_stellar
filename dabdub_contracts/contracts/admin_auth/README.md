# admin_auth

Role-based authorization registry used by the other contracts in this workspace to gate privileged operations.

## Purpose

`admin_auth` maintains the set of addresses that are allowed to perform administrative actions (e.g. pausing, upgrading, changing parameters) in dependent contracts. Dependent contracts query this contract instead of hard-coding admin addresses, so admin membership can be rotated in one place.

## Storage

- Admin set: the collection of addresses currently holding admin rights.
- Owner / super-admin: the address permitted to add or remove admins.
- Admin audit log: an append-only, ordered log of sensitive admin operations, stored under a dedicated `admin_audit_log` storage key.

(Exact storage keys are defined in `src/lib.rs`; this section is a summary, not a normative spec.)

## Admin authentication mechanism

Admin credentials are **not** stored as raw public keys. When an admin is added, the contract derives a credential from the caller's public key and stores only that derived value. Authentication derives the caller's credential the same way and compares it against the stored derived credentials.

- **Derivation:** the stored credential is a hash of the public key (see `src/lib.rs` for the exact derivation used). The raw public key is never persisted, so a leaked storage snapshot does not directly expose admin keys.
- **Comparison:** `is_admin` / privileged operations derive the caller's credential and check it against the set of stored derived credentials. A caller is authorized only if its derived credential matches an active entry.
- **Rotation:** because credentials are derived deterministically, a new key can be added (its derived credential stored) before the old one is removed. Both keys are valid during the overlap window, so rotation happens without downtime.

## Role hierarchy

Roles are ordered from least to most privileged:

```
VIEWER < OPERATOR < ADMIN < SUPERADMIN
```

A role inherits every permission granted to the roles below it. `has_permission(account, permission)` resolves the account's role and checks the permission against that role **and all lower roles in the hierarchy**, so a `SUPERADMIN` implicitly satisfies `ADMIN`, `OPERATOR`, and `VIEWER` permissions. Conversely, an `ADMIN` does **not** satisfy permissions that are granted only to `SUPERADMIN`.

## Events

- Admin added — emitted when a new admin address is granted rights.
- Admin removed — emitted when an admin address is revoked.
- `AdminAuthFailed { caller, function, timestamp }` — emitted on every failed admin authentication attempt. `caller` is the address that attempted the privileged operation, `function` identifies the operation being attempted, and `timestamp` is the ledger timestamp at the time of failure.

Consumers should index these events to keep off-chain views of admin membership in sync.

### Auth failure monitoring

Because failed admin authentication attempts emit `AdminAuthFailed`, brute-force attempts against admin keys are visible on-chain. Backends should index these events and alert when the number of failures from a single `caller` (or against a single `function`) crosses a configured threshold within a time window. Successful authentication does not emit this event, so the stream is a clean signal of rejected attempts.

## Admin operations

- `add_admin` — owner-only; derives and stores the credential for an address.
- `remove_admin` — owner-only; revokes admin rights from an address.
- `is_admin` — read-only check used by dependent contracts; derives the caller's credential and compares against stored derived credentials.
- `has_permission` — read-only check that walks the role hierarchy (see above) to decide whether an account holds a given permission.

## Key rotation

Admin key rotation is performed by adding the new admin key and then removing the old one. Because multiple admin keys are supported simultaneously, both keys are authorized during the overlap window, so rotation requires no downtime. The end-to-end rotation flow is covered by tests:

1. An admin action performed with the old key succeeds.
2. The new admin key is added (old key still active).
3. An admin action performed with the new key succeeds while the old key remains valid.
4. The old admin key is removed.
5. An admin action performed with the new key still succeeds.
6. An admin action performed with the old key fails after removal.

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
