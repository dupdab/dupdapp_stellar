# DupDub Stellar Contracts

> Soroban smart contracts powering the DupDub crypto-to-fiat settlement platform on Stellar.

## Overview

This repository contains the Soroban (Stellar smart contract platform) contracts that back DupDub's on-chain settlement flow — payment escrow, merchant registry, fee handling, and related infrastructure. It does **not** contain the backend API or frontend application; those live in their own repositories:

- **Backend (Settlement API)** — `dupdap-backend`
- **Frontend (Merchant & Customer Portal)** — `dupdap-frontend`

API work tracked in this repository is implemented in the backend; see [the backend API implementation note](docs/backend-api-implementation.md).

## Contracts

All contracts live under [`dabdub_contracts/contracts/`](dabdub_contracts/contracts/):

| Contract | Purpose | Coverage |
|---|---|---|
| `admin_timelock` | Timelocked admin actions | [![admin_timelock](https://codecov.io/gh/dupdab/dupdapp_stellar/branch/main/graph/badge.svg?flag=admin_timelock)](https://codecov.io/gh/dupdab/dupdapp_stellar) |
| `batch_payments` | Batched payment processing | [![batch_payments](https://codecov.io/gh/dupdab/dupdapp_stellar/branch/main/graph/badge.svg?flag=batch_payments)](https://codecov.io/gh/dupdab/dupdapp_stellar) |
| `fee_calculator` | Fee computation logic | [![fee_calculator](https://codecov.io/gh/dupdab/dupdapp_stellar/branch/main/graph/badge.svg?flag=fee_calculator)](https://codecov.io/gh/dupdab/dupdapp_stellar) |
| `fee_distributor` | Fee distribution to recipients | [![fee_distributor](https://codecov.io/gh/dupdab/dupdapp_stellar/branch/main/graph/badge.svg?flag=fee_distributor)](https://codecov.io/gh/dupdab/dupdapp_stellar) |
| `liquidity_router` | Liquidity routing for settlements | [![liquidity_router](https://codecov.io/gh/dupdab/dupdapp_stellar/branch/main/graph/badge.svg?flag=liquidity_router)](https://codecov.io/gh/dupdab/dupdapp_stellar) |
| `merchant_registry` | Merchant registration and metadata | [![merchant_registry](https://codecov.io/gh/dupdab/dupdapp_stellar/branch/main/graph/badge.svg?flag=merchant_registry)](https://codecov.io/gh/dupdab/dupdapp_stellar) |
| `multisig_admin` | Multi-signature admin control | [![multisig_admin](https://codecov.io/gh/dupdab/dupdapp_stellar/branch/main/graph/badge.svg?flag=multisig_admin)](https://codecov.io/gh/dupdab/dupdapp_stellar) |
| `payment_escrow` | Escrow for customer payments (approve → deposit → settle) | [![payment_escrow](https://codecov.io/gh/dupdab/dupdapp_stellar/branch/main/graph/badge.svg?flag=payment_escrow)](https://codecov.io/gh/dupdab/dupdapp_stellar) |
| `payment_request` | Payment request lifecycle | [![payment_request](https://codecov.io/gh/dupdab/dupdapp_stellar/branch/main/graph/badge.svg?flag=payment_request)](https://codecov.io/gh/dupdab/dupdapp_stellar) |
| `rbac_access` | Role-based access control | [![rbac_access](https://codecov.io/gh/dupdab/dupdapp_stellar/branch/main/graph/badge.svg?flag=rbac_access)](https://codecov.io/gh/dupdab/dupdapp_stellar) |
| `reconciliation` | Settlement reconciliation | [![reconciliation](https://codecov.io/gh/dupdab/dupdapp_stellar/branch/main/graph/badge.svg?flag=reconciliation)](https://codecov.io/gh/dupdab/dupdapp_stellar) |
| `settlement_ledger` | On-chain settlement record-keeping | [![settlement_ledger](https://codecov.io/gh/dupdab/dupdapp_stellar/branch/main/graph/badge.svg?flag=settlement_ledger)](https://codecov.io/gh/dupdab/dupdapp_stellar) |
| `slippage_protection` | Slippage guards for conversions | [![slippage_protection](https://codecov.io/gh/dupdab/dupdapp_stellar/branch/main/graph/badge.svg?flag=slippage_protection)](https://codecov.io/gh/dupdab/dupdapp_stellar) |
| `stellar_confirmations` | Confirmation tracking for Stellar transactions | [![stellar_confirmations](https://codecov.io/gh/dupdab/dupdapp_stellar/branch/main/graph/badge.svg?flag=stellar_confirmations)](https://codecov.io/gh/dupdab/dupdapp_stellar) |

## Payment escrow lifetime and storage

`DEFAULT_MAX_TTL_LEDGERS` is 518,400 ledgers: approximately 30 days at an
assumed five seconds per ledger. It is a default upper bound on a deposit's
requested lifetime, balancing a settlement window against locked capital and
operational exposure. It is not a regulatory retention requirement or an exact
wall-clock deadline. Choose shorter payment TTLs where the business permits;
compliance and settlement policies must be defined separately.

A deposit at ledger `L` with TTL `T` records `expiry = L + T`. Release is allowed
through that ledger; refund/expire becomes available only when the sequence is
strictly greater than expiry. Time passing alone does not transfer tokens or
change `Pending` status: someone must invoke settlement. An open dispute blocks
normal expiry/refund and requires admin resolution, so TTL is not a guarantee
that all funds leave escrow within 30 days.

**Admin configuration already exists:** `get_max_ttl_ledgers` reads the default
or override, and admin-authorized `set_max_ttl_ledgers(caller, value)` changes the
cap for future deposits and emits a `CONFIG/max_ttl` event. Existing payments keep
their original expiry. Keeping this configurable allows business-specific
settlement periods, but a compromised admin can lengthen future capital lockup;
operators should restrict and monitor admin access and review cap changes against
their policies. This change introduces no additional admin authority.

**Soroban storage TTL is separate from payment expiry.** Deposit extends the
payment's persistent entry and the escrow instance/code through `expiry + 1`,
including the first refundable ledger. A shorter subsequent deposit cannot
shorten those lifetimes. A requested TTL must leave that extra ledger within
`env.storage().max_ttl()`; deposits exceeding network retention capacity (or
integer capacity) fail before transferring funds, even if the admin cap is larger.
Network TTL limits can change, so clients should simulate deposits.

After the retained lifetime, unpaid or disputed records can be archived. Operators
must extend TTLs or restore archived payment, escrow instance/code, and any token
entries before settlement; archival does not refund or erase entitlement. Token
contracts have independent storage maintenance. Existing payments deposited before
this change need TTL extension/restoration too: updating the WASM does not rewrite
their stored entries. This retention policy is not an audit-history retention SLA.

Run the lifetime regressions with:

```sh
cd dabdub_contracts
cargo build --target wasm32v1-none --release -p payment-escrow
cargo test -p payment-escrow test_max_ttl
```

## Fee accuracy

`fee_calculator` computes fees using integer basis points:

```text
fee = amount * fee_bps / 10,000
net = amount - fee
```

`amount` and fee-tier thresholds must use the same smallest-unit denomination.
Integer division drops any fractional smallest unit, so the charged fee is
rounded down and differs from the exact percentage by less than one smallest
unit. For example, a 150 bps fee on 900 units is 13 units, with 887 units net.
Settlement records must use the exact fee and net returned by the calculator.

## WASM optimization level (`wasm-opt`)

Release builds run `wasm-opt` on each contract. The Makefile uses **`-Oz`**
(size-optimized) rather than `-O3` (speed-optimized). The tradeoff:

- **`-Oz`** minimizes WASM byte size. Smaller contracts cost less to upload,
  store, and instantiate on-chain, and stay comfortably under the 64 KB
  per-contract limit enforced by `make check-wasm-size`. The cost is slightly
  more instructions per call, which is negligible for these contracts because
  their execution is dominated by host functions (storage, token transfers,
  crypto) rather than guest CPU.
- **`-O3`** maximizes guest execution speed but produces larger WASM. For
  Soroban contracts the extra size increases upload/storage/instantiation cost
  and risks breaching the size limit, while the runtime savings are small
  relative to host-function cost.

**Chosen level: `-Oz`.** Contract execution cost is dominated by host calls,
so the size savings outweigh the marginal guest-CPU difference. `make
build-optimised` prints the resulting per-contract WASM sizes so the size
impact of the optimization level is visible in build output. If a future
contract becomes guest-CPU bound, benchmark `-O3` against `-Oz` before
switching.

## Development

### Prerequisites

- Rust with the `wasm32v1-none` target
- [`stellar-cli`](https://developers.stellar.org/docs/tools/developer-tools) (for deployment)
- `wasm-opt` (binaryen), for optimised release builds

### Rust formatting

Enable the repository-managed pre-commit hook once per clone:

```sh
git config core.hooksPath .githooks
```

The hook checks formatting with `cargo fmt --all -- --check`. To apply
formatting, run `cargo fmt --all` from `dabdub_contracts/`.

```bash
rustup target add wasm32v1-none
```

### Build & test

```bash
make build              # debug build of all contracts
make build-optimised    # release build + wasm-opt -Oz (prints WASM sizes)
make check-wasm-size    # enforce the 64 KB per-contract WASM limit
make test               # cargo test across the workspace
make deploy-testnet     # deploy and initialize contracts on testnet
```
