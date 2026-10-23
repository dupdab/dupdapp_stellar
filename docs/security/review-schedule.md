# Annual Security Review Schedule

## Review cadence

This schedule defines recurring annual security reviews for PCI and access controls, plus quarterly smart contract security reviews. Use the [contract audit checklist](contract-audit-checklist.md) to track each Soroban contract's review status and findings; all entries remain pending until evidence is recorded.

| Review activity | Frequency | Responsible party | Notes |
|---|---|---|---|
| PCI compliance validation | Annually | Security owner + engineering lead | Validate SAQ scope and supporting evidence.
| RBAC and access control audit | Annually | Security owner + platform architect | Review roles, permissions, least privilege, and sensitive route protections.
| Security policy updates | Annually | Security owner | Update process docs, incident response, and vulnerability management.
| TLS / transport security review | Annually | Infrastructure / backend team | Confirm TLS 1.2+ only, HSTS enabled, and insecure ciphers/protocols disabled.
| Dependency and patching process review | Annually | Engineering lead | Verify update cadence, patch windows, and scan coverage.
| Incident response rehearsal | Annually | Security and operations teams | Test incident response workflow and communication plan. |
| Smart contract security review | Quarterly | Contract owner + security owner | Run the per-contract checklist below for every deployed contract. |
| External smart contract audit | Annually (or before major upgrades) | Security owner | Coordinate scope, vendor, and remediation tracking. |

## Calendar guidance

- Schedule annual reviews in the first quarter of the calendar year.
- Schedule quarterly smart contract reviews at the start of each quarter (Jan, Apr, Jul, Oct).
- Perform additional reviews after major architecture changes, new payment integrations, contract upgrades, or security incidents.
- Record completed reviews in internal documentation or the project security tracker.

## Review checklist

- Confirm PCI scope documentation and payment handling assumptions.
- Verify authentication and RBAC controls for administrative and sensitive endpoints.
- Validate HTTPS/TLS enforcement across APIs and external services.
- Confirm vulnerability scanning tools are active and findings are tracked.
- Ensure incident response procedures remain up to date.

## Smart contract review items

Every quarterly contract review must cover the following contract-specific concerns:

- **Storage exhaustion checks**: Review storage layout and growth for unbounded maps/vectors, per-entry storage costs, and any loops that iterate over unbounded collections. Confirm storage keys are not colliding and that upgrades preserve layout.
- **Fee tier accuracy**: Verify fee tier thresholds, rates, and rounding match the intended specification, and that tier boundaries behave correctly at edge values.
- **Role assignments**: Confirm admin/operator/role assignments match the intended least-privilege model, that role grants/revocations are authorized, and that no stale or overly broad roles remain.

## Quarterly review checklist (per contract)

Run this checklist for each deployed contract during the quarterly review:

- [ ] Confirm the contract's current deployed address and version are recorded.
- [ ] Storage exhaustion checks: review storage growth, unbounded collections, and iteration limits.
- [ ] Fee tier accuracy: verify fee tiers, thresholds, and rounding against the specification.
- [ ] Role assignments: verify admin/operator roles and least-privilege assignments.
- [ ] Review access control on privileged entry points (pause, upgrade, admin setters).
- [ ] Confirm emergency procedures (e.g. pause/runbook) are current and tested.
- [ ] Review dependency and toolchain versions used to build the contract.
- [ ] Log all findings and action items as GitHub Issues (see below).

## Action item tracking

- After each review, log every finding and follow-up action as a GitHub Issue.
- Label action items with the relevant contract and a security label, and assign an owner and due date.
- Link the review record to the created issues so remediation can be tracked to closure.

## External audit schedule

- Commission an external smart contract audit annually, and before any major contract upgrade or new contract deployment.
- Define audit scope (contracts, commit hash, and threat model) before engaging the auditor.
- Track audit findings as GitHub Issues and confirm remediation before the next review cycle.
- Retain audit reports alongside the quarterly review records.
- The [contract audit checklist](contract-audit-checklist.md) contains the current external-audit placeholder; no report is represented as completed until a report is available and linked there.
