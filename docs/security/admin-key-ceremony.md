# Mainnet Admin Key Ceremony

This document defines the procedure for securely generating and storing the
mainnet admin keys used by `admin_timelock` and `multisig_admin`. It covers
key generation, storage, and disaster recovery.

> Scope: this ceremony applies to the admin keys that control the timelock and
the multisig admin role. It does not cover user keys or operational hot keys.

## 1. Roles

| Role | Responsibility |
| --- | --- |
| Ceremony coordinator | Runs the ceremony, enforces the script, records the transcript |
| Key custodians | Each holds one share / one HSM; minimum 3, threshold 2 |
| Independent observer | Verifies airgap, randomness, and transcript integrity |

No single person may hold enough material to reconstruct an admin key alone.

## 2. Preconditions

- [ ] Ceremony performed **offline on airgapped hardware**. No network
      interfaces (Wi-Fi, Ethernet, Bluetooth) enabled on any signing device.
- [ ] Airgap verified by the observer (physical inspection + `rfkill list`
      showing all radios blocked, or hardware with no radios at all).
- [ ] Fresh, reproducible build of the key-generation tooling, checksum
      verified against a previously published hash.
- [ ] At least two independent sources of entropy (hardware RNG + dice).
- [ ] Tamper-evident bags for HSM / share storage opened on camera.

## 3. Key generation

Admin keys MUST be generated using **multi-party computation (MPC)** or a
**hardware security module (HSM)**. Single-party software key generation is
not permitted for mainnet admin keys.

### Option A — MPC (threshold) generation

1. Each custodian runs the MPC key-generation client on their own airgapped
   device.
2. Devices exchange only public commitments (via removable media, verified by
   the observer). No private share ever leaves its device.
3. The ceremony produces a threshold public key (e.g. 2-of-3) plus one private
   share per custodian. The full private key is never assembled.
4. Each custodian seals their share in a tamper-evident bag and records the
   share's public commitment in the transcript.

### Option B — HSM generation

1. Initialize the HSM in a quorum/m-of-n admin configuration.
2. Generate the admin key **inside** the HSM; the private key is marked
   non-exportable.
3. Record the HSM public key and firmware version in the transcript.
4. Store the HSM in a physically secured location with split-knowledge
   operator credentials.

### Verification

- [ ] Public key recorded in the ceremony transcript and cross-checked by all
      participants.
- [ ] A test signature is produced and verified against the recorded public
      key before the key is used on mainnet.
- [ ] The public key is registered with `admin_timelock` / `multisig_admin`
      configuration and confirmed on-chain by the observer.

## 4. Storage

- Private shares / HSMs are stored in geographically distributed, access-
  controlled facilities.
- No share or HSM backup is ever stored on a networked machine or cloud
  storage.
- Access requires at least two custodians (split knowledge / dual control).
- The ceremony transcript (public keys, commitments, checksums, participant
  signatures) is committed to the repo under `docs/security/` after review.

## 5. Disaster recovery — lost admin keys

Use the least destructive path that restores control.

1. **Single share lost (MPC):** the remaining threshold of custodians can still
   sign. Run a share-refresh / re-sharing ceremony on airgapped hardware to
   replace the lost share; the public key is unchanged.
2. **Below threshold (MPC) or HSM lost/destroyed:** the admin key can no longer
   sign. Follow the on-chain recovery path:
   - Execute the pre-authorized recovery entrypoint on `admin_timelock`
     (time-delayed, guarded by the recovery multisig configured at deploy
     time).
   - If no recovery path is configured, the timelock delay must elapse and the
     documented emergency governance process is invoked.
3. **Compromise suspected:** treat as lost. Rotate the admin key via the
   recovery path, then re-run this ceremony for the new key.
4. **Post-incident:** record the incident, the recovery actions taken, and any
   key rotation in the ceremony transcript.

### Recovery drills

- A recovery drill MUST be performed at least once per year using test keys on
  a testnet, exercising both the share-refresh and the on-chain recovery path.
- Drill results are recorded alongside the ceremony transcript.

## 6. Ceremony transcript checklist

- [ ] Participants and roles recorded
- [ ] Airgap verification recorded
- [ ] Tooling checksums recorded
- [ ] Public key(s) and commitments recorded
- [ ] Test signature verified
- [ ] Storage locations and access controls recorded
- [ ] Transcript signed by all custodians and the observer
