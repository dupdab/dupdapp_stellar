# Changelog

All notable changes to the DupDub Soroban contracts are documented in this file.

This project adheres to [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html) for each
- `CHANGELOG.md` tracking contract version history (this file).

## How to update this changelog

Every merge to `main` that changes a contract's behaviour or bumps its version
**must** update this file in the same pull request:

1. Add your change under `## [Unreleased]` using the appropriate Keep a Changelog
   heading (`Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`).
2. For each affected contract, state the **contract name** and the contract's version. Contract versions are tracked per contract (see the
`version` returned by each contract's `version()` entry point) and bumped
independently.

Each entry records:

- **Contract** — the contract under `dupdapp_contract/contracts/` that changed.
- **Version bump** — the previous → new contract version (semver).
- **What changed** — a short description of the change.
- **Breaking changes** — any change that requires callers, stored state, or
  clients to be updated (marked explicitly, or `None`).
- **Deployment** — the deployment transaction hash on testnet and/or mainnet.

## [Unreleased]

### Added
- `CHANGELOG.md` tracking contract version history (this file).

## [Unreleased]

### Added

- _Nothing yet._

### Changed

- _Nothing yet._

### Breaking changes

- _None._

### Deployment

- Not yet deployed.

## [0.1.0] - 2026-01-01

Initial tracked baseline for all contracts. No functional changes are recorded
for this baseline entry; it establishes the version history going forward.

### Contracts

| Contract | Version | Breaking changes |
|---|---|---|
| `admin_timelock` | 0.1.0 | None |
| `batch_payments` | 0.1.0 | None |
| `fee_calculator` | 0.1.0 | None |
| `fee_distributor` | 0.1.0 | None |
| `liquidity_router` | 0.1.0 | None |
| `merchant_registry` | 0.1.0 | None |
| `multisig_admin` | 0.1.0 | None |
| `payment_escrow` | 0.1.0 | None |
| `payment_request` | 0.1.0 | None |
| `rbac_access` | 0.1.0 | None |
| `reconciliation` | 0.1.0 | None |
| `settlement_ledger` | 0.1.0 | None |
| `slippage_protection` | 0.1.0 | None |
| `stellar_confirmations` | 0.1.0 | None |

### Deployment

- Testnet: _baseline — record the deployment transaction hash here._
- Mainnet: _not yet deployed._

## How to update this changelog

Every merge to `main` that changes a contract **must** update this file in the
same pull request. Follow this process:

1. **Add an entry under `[Unreleased]`** in the appropriate section
   (`Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`).
2. **Record the contract and version bump** — e.g.
   `payment_escrow: 0.1.0 → 0.2.0`. Bump the contract's version constant in the
   same PR so the changelog and code stay in sync.
3. **Call out breaking changes explicitly.** If a change alters a public entry
   point signature, stored data layout, or authorization requirements, list it
   under `Breaking changes` and bump the major (or minor, pre-1.0) version.
4. **Reference the deployment transaction hash.** After the deploy workflow
   ([`.github/workflows/deploy.yml`](.github/workflows/deploy.yml)) runs on
   merge to `main` (testnet) or on a version tag (mainnet), add the resulting
   transaction hash under `Deployment` for that release.
5. **On release**, move the `[Unreleased]` entries into a new versioned section
   with the release date, and update the comparison links at the bottom.

Reviewers should block a contract-changing PR that does not update this file.

