# admin_auth

Role-based authorization registry used by the other contracts in this workspace to gate privileged operations.

## Purpose

`admin_auth` maintains the set of addresses that are allowed to perform administrative actions (e.g. pausing, upgrading, changing parameters) in dependent contracts. Dependent contracts query this contract instead of hard-coding admin addresses, so admin membership can be rotated in one place.

## Storage

- Admin set: the collection of addresses currently holding admin rights.
- Owner / super-admin: the address permitted to add or remove admins.

(Exact storage keys are defined in `src/lib.rs`; this section is a summary, not a normative spec.)

## Events

- Admin added — emitted when a new admin address is granted rights.
- Admin removed — emitted when an admin address is revoked.

Consumers should index these events to keep off-chain views of admin membership in sync.

## Admin operations

- `add_admin` — owner-only; grants admin rights to an address.
- `remove_admin` — owner-only; revokes admin rights from an address.
- `is_admin` — read-only check used by dependent contracts.

## Known limitations

- Admin membership is stored on-chain only; there is no built-in expiry or timelock. Pair with `admin_timelock` if delayed execution is required.
- Revoking an admin does not retroactively cancel actions already authorized by that admin.
- No pagination helper for enumerating admins; large admin sets may be expensive to read off-chain.

## Cross-references

- `admin_timelock` — schedules and delays privileged operations; typically combined with `admin_auth` for defense in depth.
- Any contract that performs privileged operations should consult `admin_auth::is_admin` rather than maintaining its own admin list.
