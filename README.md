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
cargo test -p payment-escrow test_max_ttl
```

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
make deploy-testnet     # deploy and initialize all contracts on Stellar testnet
```

### Manual testnet deployment

`make deploy-testnet` builds all contract WASMs and deploys all 17 contracts to
the public Stellar testnet. Configure the public addresses and contract
parameters listed in [deployment configuration](docs/deployment_configuration.md)
as environment variables before running it. The configured
`SOROBAN_SOURCE_ACCOUNT` (default `deployer`) must already exist in the Stellar
CLI key store, be funded on testnet, and have an address matching
`DEPLOY_ADMIN`, because deployment initializes admin-controlled contracts and
registers the resulting IDs.

The command writes each deployed contract ID to
[`deployments/testnet.json`](deployments/testnet.json) as soon as deployment
succeeds. A failed run leaves the IDs from its completed deployments available
for inspection; rerunning starts a new deployment and replaces that file. Keep
the deployer key out of environment files committed to the repository.

### Soroban contract events

New custom contract event topics use `(CONTRACT, action_name)`: an uppercase
contract identifier followed by a lowercase snake-case action. Existing topic
names remain unchanged for compatibility, and event payloads contain the
values needed to reconstruct each change. The backend indexer polls Soroban RPC and
persists raw contract events and its paging cursor in PostgreSQL. Configure
`SOROBAN_RPC_URL`, `SOROBAN_EVENT_CONTRACT_IDS` (comma-separated contract IDs),
and optionally `SOROBAN_EVENT_START_LEDGER` and
`SOROBAN_EVENT_POLL_INTERVAL_MS` in the backend environment.
Use the deployed IDs from `deployments/testnet.json` for the contract ID list.

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
