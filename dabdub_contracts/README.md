# dabdub_contracts

Soroban smart contracts for the Dabdub protocol.

## Reproducible builds

WASM output must be byte-for-byte identical across machines and CI agents so
build verification is possible. The toolchain is pinned in
[`rust-toolchain.toml`](./rust-toolchain.toml) and dependencies are locked via
`Cargo.lock`; do not bump either without re-verifying the WASM hash.

### Verification procedure

1. Ensure you are on the pinned toolchain (rustup installs it automatically
   from `rust-toolchain.toml`):

   ```sh
   rustup show active-toolchain
   ```

2. Build the contracts with the locked dependency graph:

   ```sh
   cargo build --locked --release --target wasm32-unknown-unknown
   ```

3. Compute the SHA-256 hash of each produced WASM artifact:

   ```sh
   sha256sum target/wasm32-unknown-unknown/release/*.wasm
   ```

4. Compare the hashes against the values reported by CI for the same commit.
   They must match exactly. If they differ, check that the toolchain version,
   `Cargo.lock`, and build flags are unchanged before investigating further.
