# stellar_confirmations

Soroban smart contract that tracks on-chain confirmation counts for payments and authorises settlement once a configurable threshold is reached.

## Purpose

After a payment is deposited into `payment_escrow`, the NestJS monitor watches for Stellar ledger closes and calls `confirm_payment` once per observed close. When the accumulated count for a given `payment_id` reaches `confirmation_count`, the contract emits a `settlement_authorised` event so the NestJS backend can trigger the fiat payout without an additional on-chain lookup.

### Is this superseded by Soroban event indexing?

Partially. Soroban event indexing (via Horizon or an off-chain indexer) can replay escrow deposit events. However `stellar_confirmations` provides an **on-chain record** of the confirmation threshold, per-payment snapshot of the required count, and the `is_settling`/`is_failed` state flags that other contracts can cross-check. It is therefore still useful as an authoritative settlement gate when strict on-chain auditability is required. If your deployment relies solely on off-chain indexing and event replay you may not need this contract — discuss with the team before removing it.

## What is confirmed

`confirm_payment` is called once per **Stellar ledger close** that the NestJS monitor observes for a given payment. It does not verify the ledger contents on-chain; it trusts the caller (the admin / monitor address) to supply a monotonically increasing `ledger_seq`. If a `payment_escrow` contract is linked via `set_payment_escrow_contract`, the amount and merchant are cross-checked against the escrow record at the threshold-crossing call.

## Confirmation threshold

The global `confirmation_count` is set at construction. It can be updated by the admin via `set_confirmation_count`. The required count for a payment is **snapshotted at its first confirmation** so that mid-flight changes to the global count do not retroactively alter an in-progress payment's threshold.

## Who calls this contract

| Caller | Function |
|---|---|
| Admin / NestJS monitor | `confirm_payment`, `fail_confirmation` |
| Admin | `set_confirmation_count`, `set_payment_escrow_contract` |
| Any (read-only) | `get_payment_confirmations`, `is_settling`, `is_failed`, `get_confirmation_count`, `get_payment_escrow_contract` |

## Interface

```rust
fn __constructor(admin: Address, confirmation_count: u32) -> ();

// Called once per ledger close for a payment.
fn confirm_payment(
    caller: Address,
    payment_id: BytesN<32>,
    ledger_seq: u32,
    amount: i128,
    merchant: Address,
) -> u32;  // returns new confirmation count

// Mark a sub-threshold payment as permanently failed.
fn fail_confirmation(caller: Address, payment_id: BytesN<32>) -> ();

// Admin setters
fn set_confirmation_count(caller: Address, count: u32) -> ();
fn set_payment_escrow_contract(caller: Address, contract: Option<Address>) -> ();

// Read-only
fn get_confirmation_count() -> u32;
fn get_payment_escrow_contract() -> Option<Address>;
fn get_payment_confirmations(payment_id: BytesN<32>) -> u32;
fn is_settling(payment_id: BytesN<32>) -> bool;
fn is_failed(payment_id: BytesN<32>) -> bool;
```

## Payment lifecycle

```
Initial → confirming (n < required) → settling (n >= required)
                                    ↘ failed (admin marks as failed)
```

Both `settling` and `failed` are **terminal states**. A settling payment cannot be failed; a failed payment cannot accumulate further confirmations.

## Events

| Topic | Data |
|---|---|
| `("STELLAR_CONFS", "confirmation_recorded")` | `{ payment_id, ledger_seq, confirmations, required }` |
| `("STELLAR_CONFS", "settlement_authorised")` | `{ payment_id, ledger_seq, amount, merchant }` |
| `("STELLAR_CONFS", "confirmation_failed")` | `{ payment_id, confirmations, required }` |

## Integration with payment_escrow

When a `payment_escrow` contract is linked:

```rust
set_payment_escrow_contract(admin, Some(escrow_address));
```

The threshold-crossing `confirm_payment` call cross-checks `amount` and `merchant` against the live escrow record. This closes the gap where a compromised monitor could fabricate settlement events.

See `src/test.rs` for unit tests covering all states and edge cases.

## Setup

```bash
cd dabdub_contracts
cargo test -p stellar_confirmations
```
