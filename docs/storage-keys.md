# Storage Keys

This document enumerates every `DataKey` variant defined by each contract, the
Soroban storage type it is written to, the Rust value type stored under it, and
its TTL behavior. Keep this table in sync whenever a new `DataKey` variant is
added to a contract.

## TTL notes

- **instance** — stored in the contract instance entry. The instance entry's
  TTL is extended whenever the contract is invoked (subject to the network's
  instance TTL bump policy); no per-key bump is required.
- **persistent** — stored in a persistent entry. Persistent entries must be
  bumped explicitly (via `extend_ttl`) or they can be archived; the contracts
  below do not currently perform explicit bumps, so entries rely on the
  network's default persistent TTL policy.
- **temporary** — stored in a temporary entry with a short TTL; not used by any
  contract below.
- **N/A** — the key is not written to storage (e.g. only used as a lookup
  discriminant) or the contract does not define that key.

## Keys by contract

| contract | key | storage type | value type | TTL |
| --- | --- | --- | --- | --- |
| admin_auth | `Admin` | instance | `Address` | instance TTL (bumped on invocation) |
| admin_auth | `Signer(Address)` | persistent | `bool` | persistent TTL (no explicit bump) |
| admin_auth | `SignerCount` | instance | `u32` | instance TTL (bumped on invocation) |
| admin_auth | `Threshold` | instance | `u32` | instance TTL (bumped on invocation) |
| admin_timelock | `Admin` | instance | `Address` | instance TTL (bumped on invocation) |
| admin_timelock | `Counter` | instance | `u64` | instance TTL (bumped on invocation) |
| admin_timelock | `Change(BytesN<32>)` | persistent | `ScheduledChange` | persistent TTL (no explicit bump) |
| admin_timelock | `PendingList` | instance | `Vec<BytesN<32>>` | instance TTL (bumped on invocation) |
| batch_payments | `Admin` | instance | `Address` | instance TTL (bumped on invocation) |
| batch_payments | `Paused` | instance | `bool` | instance TTL (bumped on invocation) |
| batch_payments | `BatchCount` | instance | `u64` | instance TTL (bumped on invocation) |
| batch_payments | `Batch(u64)` | persistent | `BatchRecord` | persistent TTL (no explicit bump) |
| fee_calculator | `Admin` | instance | `Address` | instance TTL (bumped on invocation) |
| fee_calculator | `FeeTier(u32)` | persistent | `FeeTier` | persistent TTL (no explicit bump) |
| fee_calculator | `DefaultFee` | instance | `i128` | instance TTL (bumped on invocation) |
| fee_distributor | `Admin` | instance | `Address` | instance TTL (bumped on invocation) |
| fee_distributor | `Recipient(Address)` | persistent | `u32` | persistent TTL (no explicit bump) |
| fee_distributor | `TotalShares` | instance | `u32` | instance TTL (bumped on invocation) |
| liquidity_router | `Admin` | instance | `Address` | instance TTL (bumped on invocation) |
| liquidity_router | `Route(BytesN<32>)` | persistent | `RouteConfig` | persistent TTL (no explicit bump) |
| liquidity_router | `RouteCount` | instance | `u64` | instance TTL (bumped on invocation) |

## Constructor vs runtime writes

| contract | key | set in constructor | set at runtime |
| --- | --- | --- | --- |
| admin_auth | `Admin` | yes | no |
| admin_auth | `Signer(Address)` | no | yes (add/remove signer) |
| admin_auth | `SignerCount` | yes | yes (add/remove signer) |
| admin_auth | `Threshold` | yes | yes (update threshold) |
| admin_timelock | `Admin` | yes | no |
| admin_timelock | `Counter` | yes | yes (`schedule_change`) |
| admin_timelock | `Change(BytesN<32>)` | no | yes (`schedule_change`, `apply_change`, `cancel_change`, `prune_change`) |
| admin_timelock | `PendingList` | yes | yes (`schedule_change`, `apply_change`, `cancel_change`) |
| batch_payments | `Admin` | yes | no |
| batch_payments | `Paused` | yes | yes (pause/unpause) |
| batch_payments | `BatchCount` | yes | yes (submit batch) |
| batch_payments | `Batch(u64)` | no | yes (submit batch) |
| fee_calculator | `Admin` | yes | no |
| fee_calculator | `FeeTier(u32)` | no | yes (set fee tier) |
| fee_calculator | `DefaultFee` | yes | yes (update default fee) |
| fee_distributor | `Admin` | yes | no |
| fee_distributor | `Recipient(Address)` | no | yes (set recipient) |
| fee_distributor | `TotalShares` | yes | yes (set recipient) |
| liquidity_router | `Admin` | yes | no |
| liquidity_router | `Route(BytesN<32>)` | no | yes (add/update route) |
| liquidity_router | `RouteCount` | yes | yes (add route) |
