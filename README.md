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
|---|---|
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

## Development

### Prerequisites

- Rust with the `wasm32v1-none` target
- [`stellar-cli`](https://developers.stellar.org/docs/tools/developer-tools) (for deployment)
- `wasm-opt` (binaryen), for optimised release builds

```bash
rustup target add wasm32v1-none
```

### Build & test

```bash
make build              # debug build of all contracts
make build-optimised    # release build + wasm-opt -Oz
make check-wasm-size    # enforce the 64 KB per-contract WASM limit
make test               # cargo test across the workspace
```

Or directly with cargo, from `dupdapp_contract/`:

```bash
cd dupdapp_contract
cargo build --target wasm32v1-none --release
cargo test
```

## CI/CD

- **CI** ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) — runs `cargo test`, coverage (tarpaulin), and a WASM build on every push/PR.
- **Deploy** ([`.github/workflows/deploy.yml`](.github/workflows/deploy.yml)) — builds and optimises WASMs, then deploys to Stellar testnet on merges to `main` and to mainnet on version tags (manual approval required).

## Monitoring

Grafana/Loki/Promtail configs under [`grafana/`](grafana/) and [`docker-compose.yml`](docker-compose.yml) are provided for local observability tooling; see [`monitoring/`](monitoring/) for uptime/status-page setup.

## Docs

- [Escrow upgrade procedure](docs/escrow_upgrade_procedure.md)
- [Security docs](docs/security/)

## Links

- **Website**: [https://dupdub.xyz](https://dupdub.xyz)
- **Documentation**: [https://docs.dupdub.xyz](https://docs.dupdub.xyz)

## Handsoff notes

<!-- handsoff-issue-1094 -->
- #1094: merchant_registry and payment_escrow: a merchant suspended after deposit is still paid out by release/release_partial

<!-- handsoff-issue-1107 -->
- #1107: payment_escrow::dispute's _reason parameter is misleadingly underscore-prefixed despite being used, and has no validation

<!-- handsoff-issue-1108 -->
- #1108: payment_escrow: dispute resolution is all-or-nothing on the remainder even after a merchant has already delivered via partial release

<!-- handsoff-issue-1113 -->
- #1113: payment_escrow: the highest-risk functions (upgrade, emergency_drain's XLM path, set_registry(None)) have the thinnest test coverage
