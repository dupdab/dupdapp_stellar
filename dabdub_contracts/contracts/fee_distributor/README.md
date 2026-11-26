# Fee Distributor Contract

`fee_distributor` pulls a positive USDC fee from its configured caller and
distributes the full amount among an on-chain list of fee recipients.

## Recipients

The constructor accepts `Vec<FeeRecipient>`:

```rust
pub struct FeeRecipient {
    pub address: Address,
    pub share_bps: u32,
}
```

The list must contain 1–10 unique addresses, each with a nonzero share, and
shares must total exactly 10,000 basis points. Each non-final recipient gets
the integer-truncated proportional amount; the final recipient receives the
remainder so no funds remain in the distributor due to rounding. Distribution
is atomic: any failed token transfer reverts the entire invocation.

Only the configured `allowed_caller` can distribute fees. The caller must
authorize the call and have sufficient token balance; `distribute` transfers
funds from that caller, not from a pooled account.

## Timelocked recipient changes

An admin can schedule a full replacement list with
`schedule_recipients(admin, recipients)`. The list is validated before being
queued. The admin can apply it with `apply_recipients(admin)` only after
17,280 ledgers (approximately 24 hours at five seconds per ledger). The pending
list and execution ledger are readable through `get_pending_recipients`.
`cancel_recipients_update(admin)` discards a pending change. Existing recipients
remain active until a scheduled update is applied.

The distributor stores the pending list itself rather than relying on an
off-chain relayer to execute an event from the generic admin timelock.

## Deployment and migration

The constructor arguments are `admin`, `recipients`, `usdc_token`, and
`allowed_caller`. Configure `FEE_DISTRIBUTOR_RECIPIENTS` as a JSON array of
objects with `address` and `share_bps` fields, for example:

```json
[
  { "address": "G…", "share_bps": 7000 },
  { "address": "G…", "share_bps": 2000 },
  { "address": "G…", "share_bps": 1000 }
]
```

This constructor and storage layout replace the legacy treasury/LP fields.
Existing instances cannot adopt the new recipient list in place; redeploy and
update dependent contract configuration using the deployment runbook.
