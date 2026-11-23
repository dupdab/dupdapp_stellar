# dabdub

A decentralized application platform with smart contracts written in Rust (Soroban).

## Repository layout

- `dabdub_contracts/` — Soroban smart contracts (Rust workspace)
- `frontend/` — web application
- `backend/` — API services

## Contracts

All contract crates live under `dabdub_contracts/` and are built with Cargo.

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
