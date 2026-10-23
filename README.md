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

## Contract versioning

Dependent contracts verify each other's version before calling across contract
boundaries. `payment_escrow` reads `merchant_registry`'s `version()` and panics
with a clear error if the registry is older than the escrow's
`MIN_COMPATIBLE_VERSION` constant. This prevents a major registry upgrade that
changes the `is_approved` ABI from silently breaking escrow settlement.

### Bumping a contract version

When you change a contract's public ABI (function signatures, argument types,
return types, or storage layout that other contracts read):

1. **Bump the contract's `version()`** — increment the value returned by the
   contract's `version()` entry point (e.g. `merchant_registry`).
2. **Update dependents' `MIN_COMPATIBLE_VERSION`** — for every contract that
   calls into the changed contract, raise its `MIN_COMPATIBLE_VERSION` constant
   to the new version if the change is breaking. Leave it unchanged for
   backward-compatible additions.
3. **Update the version check tests** — adjust the dependent contract's tests so
   they cover both the compatible and incompatible paths.
4. **Deploy in dependency order** — deploy the upgraded dependency first, then
   the dependents, so the version check passes at runtime.
5. **Document the change** — note the version bump and any ABI changes in the
   PR description and, for breaking changes, in
   [`docs/escrow_upgrade_procedure.md`](docs/escrow_upgrade_procedure.md).

## CI/CD

- **CI** ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) — runs `cargo test`, coverage (tarpaulin), and a WASM build on every push/PR.
- **Deploy** ([`.github/workflows/deploy.yml`](.github/workflows/deploy.yml)) — builds and optimises WASMs, then deploys to Stellar testnet on merges to `main` and to mainnet on version tags (manual approval required).

## Monitoring

Grafana/Loki/Promtail configs under [`grafana/`](grafana/) and [`docker-compose.yml`](docker-compose.yml) are provided for local observability tooling; see [`monitoring/`](monitoring/) for uptime/status-page setup.

## Docs

- [Mainnet deployment, recovery and on-call runbook](docs/deployment.md)
- [Contract ABIs (JSON)](docs/abi/) — machine-readable call signatures for each contract
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

<!-- handsoff-issue-1214 -->
- #1214: [P1] payment_escrow: add `release_to_customer` path for admin-approved refunds

<!-- handsoff-issue-1215 -->
- #1215: [P1] merchant_registry: add merchant metadata fields (website, contact, country)

