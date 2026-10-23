# Soroban Contract ABIs

Machine-readable ABI (JSON) for each DupDub Soroban contract, so backend and
frontend developers can understand call signatures without reading Rust source.

Each file is generated from the contract's public interface with
[`stellar contract inspect`](https://developers.stellar.org/docs/tools/developer-tools):

```bash
stellar contract inspect --wasm target/wasm32v1-none/release/<contract>.wasm --output json
```

| Contract | ABI |
|---|---|
| `admin_auth` | [`admin_auth.json`](admin_auth.json) |
| `admin_timelock` | [`admin_timelock.json`](admin_timelock.json) |
| `batch_payments` | [`batch_payments.json`](batch_payments.json) |
| `fee_calculator` | [`fee_calculator.json`](fee_calculator.json) |
| `fee_distributor` | [`fee_distributor.json`](fee_distributor.json) |
| `liquidity_router` | [`liquidity_router.json`](liquidity_router.json) |

## Keeping ABIs up to date

ABIs are regenerated on every build. CI runs the same generation step and fails
if the committed JSON drifts from the contract source. To refresh locally:

```bash
make abi
```

Then commit the updated files under `docs/abi/`.
