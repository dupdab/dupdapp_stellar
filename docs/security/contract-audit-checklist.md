# Soroban Contract Audit Checklist

This document tracks pre-launch security reviews for every Soroban contract in
`dabdub_contracts/contracts/`. It is a review checklist, not an audit report.
No check is marked complete until a reviewer has examined the named contract,
recorded evidence, and linked any findings. “Pending” means no conclusion has
been reached; do not treat it as a pass.

## Checklist per contract

Complete each column for the contract in that row. Record reviewer, commit,
date, evidence, and remediation issue links in the review record before
changing any status.

| Contract | Re-entrancy | Integer overflow / rounding | Access control | Storage exhaustion | Event completeness |
| --- | --- | --- | --- | --- | --- |
| `admin_auth` | Pending | Pending | Pending | Pending | Pending |
| `admin_timelock` | Pending | Pending | Pending | Pending | Pending |
| `batch_payments` | Pending | Pending | Pending | Pending | Pending |
| `contract_registry` | Pending | Pending | Pending | Pending | Pending |
| `fee_calculator` | Pending | Pending | Pending | Pending | Pending |
| `fee_distributor` | Pending | Pending | Pending | Pending | Pending |
| `liquidity_router` | Pending | Pending | Pending | Pending | Pending |
| `merchant_registry` | Pending | Pending | Pending | Pending | Pending |
| `multisig_admin` | Pending | Pending | Pending | Pending | Pending |
| `payment_escrow` | Pending | Pending | Pending | Pending | Pending |
| `payment_request` | Pending | Pending | Pending | Pending | Pending |
| `platform_stats` | Pending | Pending | Pending | Pending | Pending |
| `rbac_access` | Pending | Pending | Pending | Pending | Pending |
| `reconciliation` | Pending | Pending | Pending | Pending | Pending |
| `settlement_ledger` | Pending | Pending | Pending | Pending | Pending |
| `slippage_protection` | Pending | Pending | Pending | Pending | Pending |
| `stellar_confirmations` | Pending | Pending | Pending | Pending | Pending |

For each category, record the test or source evidence reviewed and any
limitations. In particular:

- **Re-entrancy:** identify every cross-contract call and verify state updates,
  authorization, and external effects are safe under nested calls.
- **Integer overflow / rounding:** inspect checked arithmetic, casts, fee
  rounding, counter bounds, and boundary-value tests.
- **Access control:** enumerate privileged entry points and verify signer,
  role, multisig, timelock, and emergency assumptions.
- **Storage exhaustion:** review unbounded collections, persistent-entry
  growth, TTL extension, archival, and worst-case loops.
- **Event completeness:** compare every externally relevant state transition
  with emitted topics and payloads consumed by indexers.

## Per-review record

Create one record for each contract and review cycle with:

- Contract, network/deployment address, source commit, and WASM hash.
- Reviewer(s), review date, tool versions, and threat-model scope.
- Evidence and result for each checklist category above.
- Finding identifiers, severity, owner, remediation status, and retest evidence.
- Any explicit exclusions and residual risks.

## External audit

**Status:** Not commissioned / report pending. No external audit report is
available for this repository as of this checklist. Before mainnet launch,
record the auditor, scope, audited commit and WASM hashes, report date, and a
link to the published or access-controlled report here. Track and retest every
finding before release; do not treat this placeholder as evidence of an audit.
