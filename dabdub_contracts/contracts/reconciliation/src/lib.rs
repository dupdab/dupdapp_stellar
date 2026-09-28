#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, BytesN, Env, Vec};

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct MerkleProofNode {
    pub sibling: BytesN<32>,
    pub is_left: bool,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ReconciliationBatch {
    pub merkle_root: BytesN<32>,
    pub submitted_at: u64,
    pub submitted_ledger: u32,
}

/// A batch together with the ID it is stored under. Returned by the
/// history-aware accessors so a caller can record which root a proof was
/// verified against.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct StoredBatch {
    pub batch_id: u32,
    pub batch: ReconciliationBatch,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    /// The most recently submitted batch. Retained so `get_current_batch` and
    /// the original `verify_settlement` keep working unchanged.
    CurrentBatch,
    /// A batch by its ID. Every submitted batch is kept under its own key, so
    /// a proof generated for an earlier cycle stays verifiable after later
    /// batches land.
    Batch(u32),
    /// ID that will be assigned to the next submitted batch. Absent until the
    /// first submission, which takes ID 0.
    NextBatchId,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ReconciliationSubmittedEvent {
    pub merkle_root: BytesN<32>,
    pub submitted_at: u64,
    pub submitted_ledger: u32,
    /// ID this batch was stored under. An indexer needs it to ask for the
    /// historical root later.
    pub batch_id: u32,
}

#[contract]
pub struct ReconciliationContract;

#[contractimpl]
impl ReconciliationContract {
    pub fn __constructor(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    /// Records a new reconciliation batch and returns the ID it was stored
    /// under.
    ///
    /// Every batch is kept under its own `DataKey::Batch(id)` in addition to
    /// the `CurrentBatch` pointer. Overwriting a single slot, as this did
    /// before, made a proof unverifiable the moment the next cycle landed even
    /// though it was validly included at the time.
    ///
    /// Batches are written to persistent storage: instance storage is bounded
    /// and shares one TTL, so a growing history stored there would eventually
    /// stop the contract from functioning. The `CurrentBatch` pointer stays in
    /// instance storage, where it already was.
    pub fn submit_merkle_root(env: Env, caller: Address, merkle_root: BytesN<32>) -> u32 {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let batch = ReconciliationBatch {
            merkle_root: merkle_root.clone(),
            submitted_at: env.ledger().timestamp(),
            submitted_ledger: env.ledger().sequence(),
        };

        let batch_id: u32 = env
            .storage()
            .instance()
            .get(&DataKey::NextBatchId)
            .unwrap_or(0);

        env.storage().persistent().set(&DataKey::Batch(batch_id), &batch);
        env.storage().instance().set(&DataKey::CurrentBatch, &batch);
        env.storage()
            .instance()
            .set(&DataKey::NextBatchId, &(batch_id + 1));

        env.events().publish(
            ("RECONCILIATION", "submitted"),
            ReconciliationSubmittedEvent {
                merkle_root,
                submitted_at: batch.submitted_at,
                submitted_ledger: batch.submitted_ledger,
                batch_id,
            },
        );

        batch_id
    }

    /// Verifies `proof` for `payment_id` against the root of batch `batch_id`.
    ///
    /// Returns `true` when the proof is valid — the opposite of the older
    /// [`Self::verify_settlement`], whose inverted return is a documented
    /// footgun. Prefer this function.
    ///
    /// Because the root is addressed by ID, a proof stays verifiable for as
    /// long as its batch is retained, rather than only until the next
    /// reconciliation cycle.
    ///
    /// # Panics
    ///
    /// If no batch is stored under `batch_id`. A missing batch is not the same
    /// as an invalid proof, and returning `false` for both would let a caller
    /// mistake "this root is gone" for "this payment was not settled".
    pub fn verify_settlement_proof(
        env: Env,
        batch_id: u32,
        payment_id: BytesN<32>,
        proof: Vec<MerkleProofNode>,
    ) -> bool {
        let batch: ReconciliationBatch = env
            .storage()
            .persistent()
            .get(&DataKey::Batch(batch_id))
            .expect("No reconciliation batch with that ID");

        Self::compute_root(&env, &payment_id, &proof) == batch.merkle_root
    }

    /// Returns `true` when a mismatch is detected, `false` when proof is valid.
    ///
    /// Verifies against the **latest** batch only, so a proof from an earlier
    /// reconciliation cycle reports a mismatch even though it was validly
    /// included. Its return value is also inverted relative to its name.
    ///
    /// Both behaviours are preserved for existing callers. New integrations
    /// should use [`Self::verify_settlement_proof`], which takes the batch ID
    /// the proof was generated against and returns `true` for a valid proof.
    #[deprecated(
        note = "inverted return value and latest-batch-only semantics — use verify_settlement_proof instead"
    )]
    pub fn verify_settlement(env: Env, payment_id: BytesN<32>, proof: Vec<MerkleProofNode>) -> bool {
        let batch: ReconciliationBatch = env
            .storage()
            .instance()
            .get(&DataKey::CurrentBatch)
            .expect("No reconciliation batch submitted");

        Self::compute_root(&env, &payment_id, &proof) != batch.merkle_root
    }

    pub fn get_current_batch(env: Env) -> Option<ReconciliationBatch> {
        env.storage().instance().get(&DataKey::CurrentBatch)
    }

    /// Returns the batch stored under `batch_id`, or `None` if there is none.
    pub fn get_batch(env: Env, batch_id: u32) -> Option<ReconciliationBatch> {
        env.storage().persistent().get(&DataKey::Batch(batch_id))
    }

    /// Returns the latest batch together with its ID.
    ///
    /// The ID is what a caller needs in order to verify a proof against this
    /// root later, once further batches have been submitted.
    pub fn get_latest_stored_batch(env: Env) -> Option<StoredBatch> {
        let next_id: u32 = env.storage().instance().get(&DataKey::NextBatchId)?;
        if next_id == 0 {
            return None;
        }
        let batch_id = next_id - 1;
        env.storage()
            .persistent()
            .get(&DataKey::Batch(batch_id))
            .map(|batch| StoredBatch { batch_id, batch })
    }

    /// Number of batches submitted so far. The valid IDs are `0..batch_count`.
    pub fn batch_count(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::NextBatchId)
            .unwrap_or(0)
    }

    /// Walks `proof` up from the leaf for `payment_id` and returns the root it
    /// computes. Shared by both verifiers so they cannot drift apart.
    fn compute_root(
        env: &Env,
        payment_id: &BytesN<32>,
        proof: &Vec<MerkleProofNode>,
    ) -> BytesN<32> {
        let mut current = Self::hash_leaf(env, payment_id);
        for i in 0..proof.len() {
            let node = proof.get(i).unwrap();
            current = if node.is_left {
                Self::hash_pair(env, &node.sibling, &current)
            } else {
                Self::hash_pair(env, &current, &node.sibling)
            };
        }
        current
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if &admin != caller {
            panic!("Not admin");
        }
    }

    fn hash_leaf(env: &Env, payment_id: &BytesN<32>) -> BytesN<32> {
        let id_arr = payment_id.to_array();
        env.crypto().sha256(&Bytes::from_slice(env, &id_arr)).into()
    }

    fn hash_pair(env: &Env, left: &BytesN<32>, right: &BytesN<32>) -> BytesN<32> {
        let left_arr = left.to_array();
        let right_arr = right.to_array();

        let mut combined = [0u8; 64];
        combined[..32].copy_from_slice(&left_arr);
        combined[32..].copy_from_slice(&right_arr);

        env.crypto().sha256(&Bytes::from_slice(env, &combined)).into()
    }
}
