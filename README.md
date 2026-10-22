# DupDub

DupDub is a decentralized platform for content creators and their communities.

## Repository layout

- `dupdub_contracts/` — Soroban smart contracts workspace (Cargo workspace)
- `contracts/` — per-contract crates and documentation
- `scripts/` — deployment and verification helpers
- `.github/workflows/` — CI and deployment pipelines

## Smart contracts

The Soroban contracts live in the `dupdub_contracts` workspace directory. Build and test them with:

```bash
cd dupdub_contracts
cargo test
```

## Development

See the individual contract READMEs under `contracts/` for details on building, testing, and deploying each contract.
