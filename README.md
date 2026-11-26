# dabdub

A decentralized application platform with smart contracts written in Rust (Soroban).

## Repository layout

- `dabdub_contracts/` — Soroban smart contracts (Rust workspace)
- `frontend/` — web application
- `backend/` — API services

## Contracts

All contract crates live under `dabdub_contracts/` and are built with Cargo.

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

### Reproducible builds and `Cargo.lock`

`dabdub_contracts/Cargo.lock` **is committed** to this repository. This pins every
transitive dependency to an exact version so that `cargo build` and `cargo test`
resolve the same dependency graph on every machine and in CI.

CI builds and tests the contracts with the `--locked` flag, which fails the build
if `Cargo.lock` is missing or out of date with `Cargo.toml`. Do not delete or
gitignore the lock file.

### Updating the lock file

When you intentionally change a dependency (add/remove/upgrade a crate in any
`Cargo.toml` under `dabdub_contracts/`), regenerate the lock file and commit it
in the same change:

```sh
cd dabdub_contracts
cargo update            # or: cargo update -p <crate> --precise <version>
cargo build --locked    # verify the lock file is consistent
cargo test --locked
```

Commit the updated `Cargo.lock` together with the `Cargo.toml` change. PRs that
modify dependencies without updating `Cargo.lock` will fail CI.

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
cargo build --target wasm32v1-none --release -p payment-escrow
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

<!-- handsoff-issue-1563 -->
- #1563: testing: reconciliation contract not tested with intentionally corrupted state

<!-- handsoff-issue-1533 -->
- #1533: rbac_access: role check not used by all contracts — payment_escrow bypasses RBAC for some functions

<!-- handsoff-issue-1547 -->
- #1547: slippage_protection: no event when slippage limit exceeded — silent failure

<!-- handsoff-issue-1561 -->
- #1561: testing: slippage_protection not tested with real AMM price calculations

<!-- handsoff-issue-1525 -->
- #1525: reconciliation: no mechanism to report discrepancies on-chain — only logs them

<!-- handsoff-issue-1526 -->
- #1526: reconciliation: reconciliation not protected against running during active settlements
