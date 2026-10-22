# DupDub Stellar Contracts

> Soroban smart contracts powering the DupDub crypto-to-fiat settlement platform on Stellar.

## Overview

This repository contains the Soroban (Stellar smart contract platform) contracts that back DupDub's on-chain settlement flow — payment escrow, merchant registry, fee handling, and related infrastructure. It does **not** contain the backend API or frontend application; those live in their own repositories:

- **Backend (Settlement API)** — `dupdap-backend`
- **Frontend (Merchant & Customer Portal)** — `dupdap-frontend`

API work tracked in this repository is implemented in the backend; see [the backend API implementation note](docs/backend-api-implementation.md).

## Contracts

All contracts live under [`dupdapp_contract/contracts/`](dupdapp_contract/contracts/):

| Contract | Purpose |
|---|---|
| `admin_timelock` | Timelocked admin actions |
| `batch_payments` | Batched payment processing |
| `fee_calculator` | Fee computation logic |
| `fee_distributor` | Fee distribution to recipients |
| `liquidity_router` | Liquidity routing for settlements |
| `merchant_registry` | Merchant registration and metadata |
| `multisig_admin` | Multi-signature admin control |
| `payment_escrow` | Escrow for customer payments (approve → deposit → settle) |
| `payment_request` | Payment request lifecycle |
| `rbac_access` | Role-based access control |
| `reconciliation` | Settlement reconciliation |
| `settlement_ledger` | On-chain settlement record-keeping |
| `slippage_protection` | Slippage guards for conversions |
| `stellar_confirmations` | Confirmation tracking for Stellar transactions |

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

### Running a local Soroban testnet

A Docker-based local Soroban testnet is provided via the [`stellar/quickstart`](https://hub.docker.com/r/stellar/quickstart) container, defined in [`docker-compose.yml`](docker-compose.yml).

**Prerequisites**

- Docker with the Compose plugin (`docker compose`)
- [`stellar-cli`](https://developers.stellar.org/docs/tools/developer-tools) on your `PATH`
- Rust with the `wasm32v1-none` target (see above)

**Start / stop**

```bash
make local-testnet-start   # bring up the quickstart container
make local-testnet-stop    # tear it down
```

Once running, the local network exposes:

- **Soroban RPC**: `http://localhost:8000/soroban/rpc`
- **Horizon**: `http://localhost:8000`
- **Friendbot**: `http://localhost:8000/friendbot`

The quickstart container uses the standard local network passphrase `Standalone Network ; February 2017` and a pre-funded `admin` account.

**Deploy contracts**

With the testnet running, deploy the contracts against it:

```bash
./scripts/deploy-local.sh
```

The script builds the `dabdub_contracts` WASM artifacts and deploys them to the local RPC endpoint using the quickstart default network passphrase and funded account.

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
