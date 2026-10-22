# Escrow upgrade procedure

The authoritative procedure is [Mainnet deployment and incident runbook](deployment.md#escrow-compatible-code-replacement-at-the-same-id).

The current escrow ABI uses `schedule_upgrade(caller, new_wasm_hash)`,
`get_pending_upgrade()`, `cancel_upgrade(caller)` and `apply_upgrade(caller)`.
It has no immediate `upgrade` entrypoint. The default timelock is 17,280 ledgers.
Verify the live executable/ABI and required admin authorization before acting.

Upload the reviewed compatible WASM, schedule its hash, verify the pending hash
and apply ledger, wait for that ledger, then apply with admin authorization.
Verify the executable hash and payment/storage invariants afterwards.
`get_version` counts upgrades and is not the code release identity.

Code can be replaced with an earlier compatible WASM through an available
upgrade path, but storage migrations and past transfers are not reversed.
Do not use `stellar contract deploy` for an in-place rollback: it creates a new
instance with separate storage. See the linked runbook for commands, compatibility
checks, cancellation, forward-fix alternatives and emergency containment.
