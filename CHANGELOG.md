# Changelog

All notable changes to the DupDub Soroban contracts are documented in this file.

This project adheres to [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html) for each
contract's on-chain version.

Each entry records the **contract name**, the **version bump**, **what changed**,
any **breaking changes**, and the **deployment transaction hash** on testnet
and/or mainnet where applicable.

## [Unreleased]

### Added
- `CHANGELOG.md` tracking contract version history (this file).

## How to update this changelog

Every merge to `main` that changes a contract's behaviour or bumps its version
**must** update this file in the same pull request:

1. Add your change under `## [Unreleased]` using the appropriate Keep a Changelog
   heading (`Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`).
2. For each affected contract, state the **contract name** and the **version
   bump** (e.g. `payment_escrow 0.1.0 → 0.2.0`).
3. Call out **breaking changes** explicitly — storage layout changes, changed
   function signatures, or altered authorisation requirements.
4. After deploying, add the **deployment transaction hash** for testnet and/or
   mainnet next to the version.
5. On release, move the `Unreleased` entries into a new versioned section with
   the release date.

PRs that change contract code without a corresponding changelog entry should be
requested to add one during review.

## [0.1.0] - 2026-01-01

### Added
- Initial tracked release of all contracts under `dupdapp_contract/contracts/`:
  `admin_timelock`, `batch_payments`, `fee_calculator`, `fee_distributor`,
  `liquidity_router`, `merchant_registry`, `multisig_admin`, `payment_escrow`,
  `payment_request`, `rbac_access`, `reconciliation`, `settlement_ledger`,
  `slippage_protection`, `stellar_confirmations`.

### Breaking changes
- None (initial release).

### Deployments
- Testnet: _pending_
- Mainnet: _pending_
