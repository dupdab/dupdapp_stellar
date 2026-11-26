# Storage Keys Reference

This file lists every storage key used by the DabDub Soroban contracts, its storage type, and the rationale. See `storage-strategy.md` for the policy behind these choices.

## settlement_ledger

| Key | Type | Value | Notes |
|-----|------|-------|-------|
| `Admin` | Instance | `Address` | Privileged caller; never expires with the contract. |
| `Config` | Instance | `Config` struct | Fee and currency parameters. |
| `SettlementCount` | Instance | `u64` | Monotonic settlement id counter. |
| `Settlement(u64)` | Persistent | `Settlement` struct | Financial record; retained for audit. TTL bumped on access. |
| `PaymentIndex(BytesN<32>)` | Persistent | `u64` settlement id | Maps external payment id to settlement. |
| `Nonce(Address, u64)` | Temporary | `bool` | Replay guard, valid only within the signature window. |
