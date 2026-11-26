# Contract Event XDR Schemas

This document describes the on-chain events emitted by each Soroban contract.
The NestJS `SorobanEventIndexerService` must be able to deserialise every event
listed here.  When an event's fields change, update this file and the
corresponding indexer test in the contract's `src/test.rs`.

---

## settlement_ledger

### `settlement_recorded`

Topic: `("SETTLEMENT_LEDGER", "settlement_recorded")`

| Field           | Soroban type    | Description                                      |
|----------------|-----------------|--------------------------------------------------|
| `payment_id`   | `BytesN<32>`    | Unique payment identifier                        |
| `merchant`     | `Address`       | Merchant receiving the settlement                |
| `amount`       | `i128`          | Gross amount (in USDC stroops)                   |
| `fee`          | `i128`          | Platform fee deducted                            |
| `net`          | `i128`          | Net remitted to merchant (`amount - fee`)        |
| `fiat_ref`     | `String`        | Bank / fiat-rail reference ID                    |
| `fiat_currency`| `String`        | ISO 4217 fiat currency code (e.g. "NGN", "KES") |

---

## fee_calculator

### `tier_applied`

Topic: `("FEE", "tier_applied")`

| Field          | Soroban type | Description                                         |
|----------------|--------------|-----------------------------------------------------|
| `merchant`     | `Address`    | Merchant the fee was calculated for                 |
| `volume_usdc`  | `i128`       | Rolling 30-day volume after this payment            |
| `fee_bps`      | `u32`        | Basis-points rate applied (e.g. 150 = 1.5%)         |
| `fee`          | `i128`       | Fee amount in USDC stroops                          |
| `net`          | `i128`       | Net amount after fee                                |

---

## merchant_registry

### `merchant_registered`

Topic: `("REGISTRY", "merchant_registered")`

| Field      | Soroban type | Description            |
|-----------|--------------|------------------------|
| `merchant` | `Address`    | Registered merchant    |
| `name`     | `String`     | Business name          |

### `merchant_updated`

Topic: `("REGISTRY", "merchant_updated")`

| Field      | Soroban type | Description         |
|-----------|--------------|---------------------|
| `merchant` | `Address`    | Updated merchant    |
| `name`     | `String`     | New business name   |

### `merchant_suspended`

Topic: `("REGISTRY", "merchant_suspended")`

| Field      | Soroban type | Description       |
|-----------|--------------|-------------------|
| `merchant` | `Address`    | Suspended merchant|

### `merchant_reactivated`

Topic: `("REGISTRY", "merchant_reactivated")`

| Field      | Soroban type | Description          |
|-----------|--------------|----------------------|
| `merchant` | `Address`    | Reactivated merchant |

### `merchant_terminated`

Topic: `("REGISTRY", "merchant_terminated")`

| Field      | Soroban type | Description          |
|-----------|--------------|----------------------|
| `merchant` | `Address`    | Terminated merchant  |

### `kyc_status_updated`

Topic: `("REGISTRY", "kyc_status_updated")`

| Field      | Soroban type | Description            |
|-----------|--------------|------------------------|
| `merchant` | `Address`    | Merchant               |
| `verified` | `bool`       | New KYC verified state |

### `fee_tier_updated`

Topic: `("REGISTRY", "fee_tier_updated")`

| Field      | Soroban type | Description                  |
|-----------|--------------|------------------------------|
| `merchant` | `Address`    | Merchant                     |
| `fee_bps`  | `u32`        | New basis-points fee rate    |

### `admin_transferred`

Topic: `("REGISTRY", "admin_transferred")`

| Field       | Soroban type | Description    |
|------------|--------------|----------------|
| `old_admin` | `Address`    | Previous admin |
| `new_admin` | `Address`    | New admin      |

---

## Notes for the indexer

- All `i128` values represent **USDC stroops** (1 USDC = 10,000,000 stroops).
- `fiat_currency` is an ISO 4217 code; historical records written before
  multi-currency support default to `"NGN"`.
- If a new field is added to any event struct, bump the version comment in
  `SorobanEventIndexerService` and update this file.
