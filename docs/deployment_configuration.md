# Deployment configuration

`.github/workflows/deploy.yml` runs automated deployments on merges to `main`
and `v*` tags. The manual `make deploy-testnet` command independently builds
and deploys all 17 current contracts to testnet. Constructor and initialization
arguments for that command are supplied as environment variables; the
automated workflow reads its own values from GitHub Actions **variables**
(Settings → Secrets and variables → Actions → Variables).

For local manual deployments, `make deploy-testnet` uses the same public
configuration values under the unprefixed names shown below. It also requires
the CLI's `SOROBAN_SOURCE_ACCOUNT` (default `deployer`) to be the
`DEPLOY_ADMIN` address, and writes IDs incrementally to
`deployments/testnet.json`.

Values are per-network, prefixed `TESTNET_` or `MAINNET_`. The deploy step
checks every required variable up front and fails naming the missing one, rather
than letting an opaque CLI error surface halfway through a partial deploy.

> Addresses and tuning values are configuration, not credentials, so they are
> variables rather than secrets. The only secrets involved remain
> `TESTNET_DEPLOY_SECRET` and `MAINNET_DEPLOY_SECRET`, the deployer keypairs.

## Required variables

| Variable (per-network prefix) | Used by | Example |
| --- | --- | --- |
| `…_DEPLOY_ADMIN` | `admin_timelock`, `merchant_registry`, `reconciliation`, `settlement_ledger`, `slippage_protection`, `stellar_confirmations`, `fee_calculator`, `fee_distributor`, `payment_escrow` | `GA…` |
| `…_RBAC_SUPER_ADMIN` | `rbac_access` (`super_admin`) | `GA…` |
| `…_XLM_TOKEN` | `payment_escrow` | `CA…` |
| `…_USDC_TOKEN` | `payment_escrow`, `fee_distributor` | `CA…` |
| `…_TREASURY` | `fee_distributor` | `GA…` |
| `…_LP_ADDRESS` | `fee_distributor` | `GA…` |
| `…_LP_SHARE_BPS` | `fee_distributor` | `2000` |
| `…_FEE_DISTRIBUTOR_ALLOWED_CALLER` | `fee_distributor` | `C…` |
| `…_FEE_TIERS` | `fee_calculator` | JSON array, see below |
| `…_MIN_FEE_BPS` | `fee_calculator` | `0` |
| `…_MULTISIG_ADMIN_1/2/3` | `multisig_admin` | `GA…` |
| `…_ESCROW_DEFAULT_TTL_LEDGERS` | `payment_escrow` | `518400` |
| `…_EMERGENCY_SIGNERS` | `payment_escrow` | JSON array, see below |
| `…_EMERGENCY_TREASURY` | `payment_escrow` | `GA…` |
| `…_EMERGENCY_COOLDOWN_LEDGERS` | `payment_escrow` | `50` |
| `…_CONFIRMATION_COUNT` | `stellar_confirmations` | `3` |
| `…_BATCH_PAYMENTS_MIN_AMOUNT` | `batch_payments` | `1` |
| `…_BATCH_PAYMENTS_MAX_AMOUNT` | `batch_payments` | `1000000000` |

### Optional

| Variable | Effect |
| --- | --- |
| `…_REGISTRY_ADDRESS` | `payment_escrow`'s `registry` argument. Left unset, it defaults to the `merchant_registry` deployed in the same run, so the two are wired together without a follow-up call. Set it to point at an already-deployed registry instead. |

## Structured values

Two constructors take container types, which the Stellar CLI expects as JSON.

`…_EMERGENCY_SIGNERS` — `Vec<Address>`:

```json
["GA…", "GB…", "GC…"]
```

`…_FEE_DISTRIBUTOR_RECIPIENTS` — `Vec<FeeRecipient>`. Shares must total exactly
10,000 basis points, addresses must be unique, and at most 10 recipients are
accepted:

```json
[
  { "address": "G…", "share_bps": 7000 },
  { "address": "G…", "share_bps": 2000 },
  { "address": "G…", "share_bps": 1000 }
]
```

`fee_distributor` recipient changes require a 17,280-ledger (~24 hour)
timelock. The admin schedules a complete replacement list; once the deadline is
reached, the admin applies it. The fee distributor and its existing deployments
must be redeployed to adopt this constructor and storage schema.

`…_FEE_TIERS` — `Vec<FeeTier>`. Field names must match the struct in
`dabdub_contracts/contracts/fee_calculator/src/lib.rs`:

```json
[
  { "threshold_usdc": 0,    "fee_bps": 150 },
  { "threshold_usdc": 1000, "fee_bps": 120 }
]
```

`…_REGISTRY_ADDRESS` is optional. `…_FEE_DISTRIBUTOR_ALLOWED_CALLER` should be
the public contract or account that is authorized to distribute calculated
fees. `…_MIN_FEE_BPS` configures the minimum rate used by `fee_calculator`.

## Contracts without constructor arguments

`contract_registry`, `batch_payments`, `liquidity_router` and `payment_request`
do not take Soroban constructor arguments. Deployment initializes them with
follow-up calls; `batch_payments` additionally requires its amount limits.

## Deploy order

The workflow/manual command deploys constructor-free contracts first, then
admin-only ones, then multi-argument ones. `merchant_registry` is deployed
before `payment_escrow` so the registry address is available to it. The manual
command also registers the IDs in `contract_registry`.

Contract IDs for every deployed contract are written to the job summary and
exposed as job outputs named `<contract>_id`.

## Related

- [`escrow_upgrade_procedure.md`](escrow_upgrade_procedure.md) — upgrading
  `payment_escrow` in place.
- [`../dabdub_contracts/contracts/payment_escrow/EMERGENCY_RUNBOOK.md`](../dabdub_contracts/contracts/payment_escrow/EMERGENCY_RUNBOOK.md)
  — incident response, including rollback.
