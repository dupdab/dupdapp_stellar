# Mainnet deployment and incident runbook

Use this runbook for every release and incident. Commands below are operator
examples, not an instruction to deploy automatically. Replace placeholders with
verified public addresses and local signing aliases; never put private keys in
commands, release tickets, or this document. Rehearse with the exact release
artifacts on testnet before using mainnet.

## What the workflow actually does

[`deploy.yml`](../.github/workflows/deploy.yml) builds the
[`dabdub_contracts`](../dabdub_contracts/Cargo.toml) workspace, optimizes each WASM
with `wasm-opt -Oz`, enforces a 65,536-byte per-file limit, and retains artifacts
as `wasm-contracts-<commit SHA>` for 30 days. A push to `main` deploys to testnet;
a `v*` tag starts the mainnet job using the `mainnet` GitHub environment.
**Required reviewers must actually be configured on that environment**; the
workflow's environment declaration alone does not enforce human approval.

The workflow calls `stellar contract deploy`: it creates new instances, IDs and
storage. It neither upgrades the live escrow nor migrates balances, open
payments, or downstream configuration. Do not tag a release expecting an
in-place upgrade. A partially failed deployment may already have created some
contracts; inventory its transaction results before retrying.

## Pre-deploy checklist

- [ ] Name the release owner, incident commander, on-call engineer, backup and
  signing custodians in the contact roster below; obtain availability acknowledgments.
- [ ] Record the reviewed commit/tag, intended network, all live contract IDs,
  current executable hashes, proposed WASM hashes, constructor values and the
  downstream services that reference each ID. Identify new deployment versus
  in-place escrow upgrade explicitly.
- [ ] Verify the current deployed ABI and admin authority against the deployed
  hash. Source on `main` is not proof of what runs on mainnet.
- [ ] Require successful CI tests, coverage and WASM-size checks for the exact
  commit. Local equivalents are `make test` and `make check-wasm-size` from the
  repository root (or `cargo test --manifest-path dabdub_contracts/Cargo.toml`).
  Do not approve a production release with unexplained failing checks.
- [ ] Rehearse constructor wiring, admin authorization, payment deposit/release,
  partial release, refund/expiry, registry gating, and recovery against testnet.
  Verify storage compatibility with representative existing payments, including
  disputed and partially released payments, before any code downgrade.
- [ ] Confirm GitHub `mainnet` environment reviewers, release-tag controls,
  `MAINNET_DEPLOY_SECRET` custody and all variables actually required by the
  workflow. Use [deployment configuration](deployment_configuration.md) as an
  inventory aid, but compare it with the current workflow, including
  `MAINNET_BATCH_PAYMENTS_MIN_AMOUNT` and `MAINNET_BATCH_PAYMENTS_MAX_AMOUNT`.
- [ ] Confirm mainnet RPC health, the passphrase
  `Public Global Stellar Network ; September 2015`, deployer funding, transaction
  budget, live admin address and all required signers. The deployer is not
  necessarily the escrow admin. Record Rust, Stellar CLI and wasm-opt versions;
  the workflow currently installs unpinned stable/latest tooling.
- [ ] Archive the exact optimized previous and candidate WASMs with SHA-256
  hashes outside expiring Actions artifacts. Preserve IDs, constructor/config
  values, payment inventory, balances, ledger references and audit evidence.
  This is recovery evidence, not a restorable blockchain snapshot.
- [ ] Establish off-chain containment controls, monitoring, stop conditions and
  the rollback/forward-fix decision owner. There is no on-chain pause switch in
  the current escrow or multisig contracts. If an enforceable on-chain pause is
  a release requirement, stop until an implemented and tested mechanism exists.

## Deployment and verification

1. Prepare an approved release record containing the checklist, contact roster,
   artifact hashes, migration plan and expected contract-ID changes. Rehearse
   containment and signer coordination before opening the deployment window.
2. For new instances, create the approved `v*` tag at the reviewed commit and
   review the mainnet environment job before approving it. For an existing
   escrow, use the schedule/apply procedure below instead of the tag deployment.
3. Preserve the Actions run URL and job summary. Independently verify every
   resulting contract ID, executable hash, constructor state, admin, token,
   registry and treasury on the intended network. Do not treat a printed ID or
   successful upload as proof of correct initialization.
4. For new instances, execute the separately approved state/funds migration and
   downstream cutover. Keep old instances and their open payments tracked; do
   not route an old payment to a new, empty contract. Deploying another WASM does
   not recover funds already held by an old instance.
5. Run the approved low-value canary and read-only payment/balance checks.
   Watch failed invocations, settlement lag, registry failures and balance
   discrepancies. On unexpected behavior, stop expansion and enter containment.
6. Close the window only after the on-call engineer and release owner confirm
   monitoring and the inventory reconcile. Record the observation period in the
   release record rather than assuming a universal safe duration.

## Can WASM be downgraded?

Soroban is not universally “upgrade is one-way.” A contract with an authorized
executable-update path can select an already uploaded, compatible earlier WASM.
That replaces code, **not ledger history or storage changes**. See Stellar's
[upgrade model](https://developers.stellar.org/docs/build/guides/conventions/upgrading-contracts)
and [WASM upload guide](https://developers.stellar.org/docs/tools/cli/cookbook/upload-wasm).

In this repository, only
[`payment_escrow`](../dabdub_contracts/contracts/payment_escrow/src/lib.rs)
calls `update_current_contract_wasm`. Its current ABI uses `schedule_upgrade`,
`get_pending_upgrade`, `cancel_upgrade`, and `apply_upgrade`; there is no current
`upgrade` entrypoint. The default delay is **17,280 ledgers**, not a guaranteed
wall-clock duration. Never bypass the delay or assume old runbook commands match
the deployed release.

### Escrow: compatible code replacement at the same ID

First stop new off-chain intake, identify the exact live hash/ABI, and have the
contract engineer approve storage compatibility. A prior WASM that cannot read
current state, removes recovery entrypoints, or reintroduces an exploit is not
a safe recovery target; prepare a compatible forward fix instead.

Use an audited local signing alias. If the admin is a multisig account or custom
contract, collect the required transaction/auth-entry signatures through the
approved signing procedure; a `multisig_admin` proposal is not that authorization.
The single-source examples below alone do not satisfy multiple signers.

```bash
# Configure and independently verify the intended network first.
stellar network add mainnet \
  --rpc-url "$MAINNET_RPC_URL" \
  --network-passphrase 'Public Global Stellar Network ; September 2015'

# Upload the exact archived compatible WASM; record its returned hash.
stellar contract upload --wasm "$RECOVERY_WASM" \
  --source "$UPLOAD_SIGNER_ALIAS" --network mainnet

stellar contract invoke --id "$LIVE_ESCROW_ID" --network mainnet -- get_admin
stellar contract invoke --id "$LIVE_ESCROW_ID" --network mainnet -- get_pending_upgrade

# After independent review of RECOVERY_WASM_HASH and required authorization:
stellar contract invoke --id "$LIVE_ESCROW_ID" \
  --source "$ADMIN_SIGNER_ALIAS" --network mainnet -- schedule_upgrade \
  --caller "$ADMIN_ADDRESS" --new_wasm_hash "$RECOVERY_WASM_HASH"

stellar contract invoke --id "$LIVE_ESCROW_ID" --network mainnet -- get_pending_upgrade

# Only once the returned apply ledger is reached, recheck the pending hash:
stellar contract invoke --id "$LIVE_ESCROW_ID" \
  --source "$ADMIN_SIGNER_ALIAS" --network mainnet -- apply_upgrade \
  --caller "$ADMIN_ADDRESS"
```

`schedule_upgrade` can replace an existing pending proposal, so compare and
record pending state before scheduling and again immediately before applying.
To cancel a bad pending upgrade, the authorized admin invokes:

```bash
stellar contract invoke --id "$LIVE_ESCROW_ID" \
  --source "$ADMIN_SIGNER_ALIAS" --network mainnet -- cancel_upgrade \
  --caller "$ADMIN_ADDRESS"
```

After applying, verify the executable hash on the live contract instance through
RPC/explorer and compare it with the archived artifact. Check balances, registry,
admin and representative `get_payment`/`get_balance` results. `get_version`
increments even when selecting older code; it is an upgrade counter, not proof
of a specific release. Reconcile transactions processed during the incident.
Code replacement cannot undo transfers or incompatible storage migrations.

### Other contracts, or an unusable upgrade/admin path

Contain off-chain traffic and plan a forward fix or replacement deployment with
explicit state/funds migration and downstream address changes. Contracts without
an upgrade path cannot be rewritten in place by uploading/deploying an older
WASM. If the live escrow has lost its usable authorized upgrade path, the same
limitation applies. Preserve the original instance and outstanding liabilities;
do not claim that repointing the UI or backend moves funds or blocks direct calls.

## Emergency containment using multisig_admin

**There is no on-chain `pause`/`unpause` entrypoint here.**
[`multisig_admin`](../dabdub_contracts/contracts/multisig_admin/src/lib.rs)
records proposals and approvals, and handles selected changes to its own admin
configuration; it does not dispatch arbitrary calls to escrow or other contracts.
An `emergency_pause` operation below is an incident approval signal, not a
contract pause. No working pause relayer is established by this runbook.

1. Page the incident commander and on-call engineer. Stop pending deployment
   approvals and off-chain new deposit/payment intake; stop automated settlement
   submitters if they would worsen the incident, retaining their queues/evidence.
   Use the deployment-specific controls recorded in the release record. Do not
   wait for on-chain signaling before urgent off-chain containment.
2. Query `get_admins` and `get_threshold` on the verified live multisig ID.
   Have distinct authorized custodians review the same incident scope and digest.
   The proposer supplies the first approval; proposals expire after 24 hours.
3. Record and approve the containment decision using local signing aliases:

   ```bash
   stellar contract invoke --id "$MULTISIG_ID" --network mainnet -- get_admins
   stellar contract invoke --id "$MULTISIG_ID" --network mainnet -- get_threshold
   stellar contract invoke --id "$MULTISIG_ID" --source "$SIGNER_ONE_ALIAS" \
     --network mainnet -- propose --caller "$SIGNER_ONE_ADDRESS" \
     --operation emergency_pause --args "$INCIDENT_DIGEST_HEX"
   # Record the returned proposal ID; the next signer independently reviews it.
   stellar contract invoke --id "$MULTISIG_ID" --network mainnet -- get_proposal \
     --proposal_id "$PROPOSAL_ID"
   stellar contract invoke --id "$MULTISIG_ID" --source "$SIGNER_TWO_ALIAS" \
     --network mainnet -- approve --caller "$SIGNER_TWO_ADDRESS" \
     --proposal_id "$PROPOSAL_ID"
   ```

   `INCIDENT_DIGEST_HEX` is the hex-encoded SHA-256 digest of the approved incident
   record, not personal data or secret recovery material. Collect further
   approvals if required by the reviewed policy. Independently inspect distinct
   approvals, expiry and the configured threshold (at least two approvals);
   **do not trust `executed=true` or an event alone**. The implementation contains
   unconditional executed-flag/event handling after its threshold branch.
4. The on-call engineer applies and verifies the off-chain controls, records
   acknowledgments and checks that new traffic/automated submissions have stopped.
   **Existing on-chain entrypoints remain callable directly.** Escalate funds at
   risk to the contract engineer and emergency signing custodians.
5. `emergency_drain` is a separate last-resort USDC transfer to the configured
   emergency treasury, not pause or rollback. It requires caller authorization,
   two distinct configured emergency signers and the configured cooldown; it
   does not drain XLM or reconcile individual payment obligations. Follow the
   [emergency drain runbook](../dabdub_contracts/contracts/payment_escrow/EMERGENCY_RUNBOOK.md)
   only after reviewing the live ABI, treasury and recovery/reimbursement plan.
6. Resume only after verified recovery, payment/balance reconciliation and
   approval by the incident commander and signing policy. Record a separate
   resume decision and restore the off-chain controls gradually with canaries;
   there is no corresponding on-chain `unpause` call.

## Deployment contact roster

No verified personal on-call contacts or pager service are published in this
repository. The release owner must fill this roster in the access-controlled
release record **before approving mainnet**, replacing every `UNASSIGNED` value
and testing reachability. Do not invent contacts or publish private phone numbers.

| Role | Primary / backup | Reachable contact and escalation |
| --- | --- | --- |
| Release owner / incident commander | UNASSIGNED / UNASSIGNED | Private incident bridge and decision authority: UNASSIGNED |
| On-call engineer during deployment | UNASSIGNED / UNASSIGNED | Pager/phone/secure chat and shift end: UNASSIGNED |
| Contract engineer | UNASSIGNED / UNASSIGNED | ABI, storage and recovery review channel: UNASSIGNED |
| Multisig custodians | Three named custodians: UNASSIGNED | Public signer-address mapping and private contact per custodian: UNASSIGNED |
| Emergency drain custodians | Configured signer set: UNASSIGNED | May differ from multisig_admin; verify quorum availability: UNASSIGNED |
| Backend/UI operator | UNASSIGNED / UNASSIGNED | Intake, submitter, routing and monitoring controls: UNASSIGNED |

Page primary and backup together for a deployment incident. If quorum, escalation
ownership or tested containment controls are unavailable, do not start/resume
mainnet rollout. GitHub issue comments are not an emergency paging channel.
Record acknowledgment times, transaction hashes, apply ledger, artifact hashes,
affected payments and final reconciliation in the incident record.
