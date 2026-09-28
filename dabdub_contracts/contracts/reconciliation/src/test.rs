#![cfg(test)]

use crate::{
    MerkleProofNode, ReconciliationContract, ReconciliationContractClient, ReconciliationSubmittedEvent,
};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    vec, Address, BytesN, Env, IntoVal, TryFromVal,
};

fn setup_env() -> (Env, ReconciliationContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(ReconciliationContract, (&admin,));
    let client = ReconciliationContractClient::new(&env, &contract_id);

    (env, client, admin)
}

fn make_id(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn hash_leaf(env: &Env, payment_id: &BytesN<32>) -> BytesN<32> { ReconciliationContract::hash_leaf(env, payment_id) }
fn hash_pair(env: &Env, left: &BytesN<32>, right: &BytesN<32>) -> BytesN<32> { ReconciliationContract::hash_pair(env, left, right) }

#[test]
fn test_admin_can_submit_merkle_root_and_store_batch() {
    let (env, client, admin) = setup_env();
    env.ledger().set_sequence_number(50);
    env.ledger().set_timestamp(1_710_000_000);

    let payment_a = make_id(&env, 1);
    let payment_b = make_id(&env, 2);
    let root = hash_pair(&env, &hash_leaf(&env, &payment_a), &hash_leaf(&env, &payment_b));

    client.submit_merkle_root(&admin, &root);
    let batch = client.get_current_batch().unwrap();

    assert_eq!(batch.merkle_root, root);
    assert_eq!(batch.submitted_ledger, 50);
    assert_eq!(batch.submitted_at, 1_710_000_000);
}

#[test]
fn test_verify_settlement_valid_proof_returns_no_mismatch() {
    let (env, client, admin) = setup_env();
    let payment_a = make_id(&env, 10);
    let payment_b = make_id(&env, 20);

    let leaf_a = hash_leaf(&env, &payment_a);
    let leaf_b = hash_leaf(&env, &payment_b);
    let root = hash_pair(&env, &leaf_a, &leaf_b);
    client.submit_merkle_root(&admin, &root);

    let proof = vec![
        &env,
        MerkleProofNode {
            sibling: leaf_b,
            is_left: false,
        },
    ];

    let mismatch = client.verify_settlement(&payment_a, &proof);
    assert!(!mismatch);
}

#[test]
fn test_verify_settlement_invalid_proof_returns_mismatch() {
    let (env, client, admin) = setup_env();
    let payment_a = make_id(&env, 11);
    let payment_b = make_id(&env, 22);
    let wrong = make_id(&env, 99);

    let leaf_a = hash_leaf(&env, &payment_a);
    let leaf_b = hash_leaf(&env, &payment_b);
    let wrong_leaf = hash_leaf(&env, &wrong);
    let root = hash_pair(&env, &leaf_a, &leaf_b);
    client.submit_merkle_root(&admin, &root);

    let invalid_proof = vec![
        &env,
        MerkleProofNode {
            sibling: wrong_leaf,
            is_left: false,
        },
    ];

    let mismatch = client.verify_settlement(&payment_a, &invalid_proof);
    assert!(mismatch);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_non_admin_cannot_submit_merkle_root() {
    let (env, client, _admin) = setup_env();
    let random = Address::generate(&env);
    let root = make_id(&env, 42);
    client.submit_merkle_root(&random, &root);
}

#[test]
fn test_admin_can_transfer_admin() {
    let (env, client, admin) = setup_env();
    let new_admin = Address::generate(&env);
    client.transfer_admin(&admin, &new_admin);
    client.submit_merkle_root(&new_admin, &make_id(&env, 9));
}

#[test]
fn test_submit_emits_reconciliation_submitted_event() {
    let (env, client, admin) = setup_env();
    env.ledger().set_sequence_number(77);
    env.ledger().set_timestamp(1_720_000_000);

    let root = make_id(&env, 7);
    client.submit_merkle_root(&admin, &root);

    let all_events = env.events().all();
    let event = all_events.last().unwrap();

    let expected_topic = ("RECONCILIATION", "submitted").into_val(&env);
    assert_eq!(event.1, expected_topic);

    let payload = ReconciliationSubmittedEvent::try_from_val(&env, &event.2).unwrap();
    assert_eq!(payload.merkle_root, root);
    assert_eq!(payload.submitted_ledger, 77);
    assert_eq!(payload.submitted_at, 1_720_000_000);
    // The ID is what an indexer needs to ask for this root again later.
    assert_eq!(payload.batch_id, 0);
}

// ---------------------------------------------------------------------------
// Historical batches (#1027)
// ---------------------------------------------------------------------------

/// Builds a two-leaf tree and returns `(root, proof_for_a)`.
fn two_leaf_tree(
    env: &Env,
    a: &BytesN<32>,
    b: &BytesN<32>,
) -> (BytesN<32>, soroban_sdk::Vec<MerkleProofNode>) {
    let leaf_a = hash_leaf(env, a);
    let leaf_b = hash_leaf(env, b);
    let root = hash_pair(env, &leaf_a, &leaf_b);
    let proof = vec![
        env,
        MerkleProofNode {
            sibling: leaf_b,
            is_left: false,
        },
    ];
    (root, proof)
}

#[test]
fn test_submit_returns_incrementing_batch_ids() {
    let (env, client, admin) = setup_env();

    assert_eq!(client.batch_count(), 0);
    assert_eq!(client.submit_merkle_root(&admin, &make_id(&env, 1)), 0);
    assert_eq!(client.submit_merkle_root(&admin, &make_id(&env, 2)), 1);
    assert_eq!(client.submit_merkle_root(&admin, &make_id(&env, 3)), 2);
    assert_eq!(client.batch_count(), 3);
}

#[test]
fn test_every_submitted_batch_is_retained() {
    let (env, client, admin) = setup_env();

    let first = make_id(&env, 1);
    let second = make_id(&env, 2);
    client.submit_merkle_root(&admin, &first);
    client.submit_merkle_root(&admin, &second);

    // Before this fix the first root was gone the moment the second landed.
    assert_eq!(client.get_batch(&0).unwrap().merkle_root, first);
    assert_eq!(client.get_batch(&1).unwrap().merkle_root, second);
}

#[test]
fn test_old_proof_still_verifies_after_a_newer_batch_lands() {
    let (env, client, admin) = setup_env();

    let payment_a = make_id(&env, 10);
    let payment_b = make_id(&env, 20);
    let (root, proof) = two_leaf_tree(&env, &payment_a, &payment_b);

    let batch_id = client.submit_merkle_root(&admin, &root);

    // A later reconciliation cycle replaces the current root.
    client.submit_merkle_root(&admin, &make_id(&env, 99));

    // The regression this issue is about: the old proof used to become
    // permanently unverifiable here.
    assert!(client.verify_settlement_proof(&batch_id, &payment_a, &proof));

    // ...and the legacy entrypoint still reports a mismatch against the
    // newest root, which is exactly why the batch-addressed one exists.
    assert!(client.verify_settlement(&payment_a, &proof));
}

#[test]
fn test_verify_settlement_proof_returns_true_for_a_valid_proof() {
    let (env, client, admin) = setup_env();

    let payment_a = make_id(&env, 30);
    let payment_b = make_id(&env, 40);
    let (root, proof) = two_leaf_tree(&env, &payment_a, &payment_b);
    let batch_id = client.submit_merkle_root(&admin, &root);

    // Note the polarity: true means verified, unlike `verify_settlement`.
    assert!(client.verify_settlement_proof(&batch_id, &payment_a, &proof));
}

#[test]
fn test_verify_settlement_proof_single_leaf_empty_proof() {
    let (env, client, admin) = setup_env();
    let payment = make_id(&env, 77);
    let root = hash_leaf(&env, &payment);
    let batch_id = client.submit_merkle_root(&admin, &root);
    let proof = vec![&env];
    assert!(client.verify_settlement_proof(&batch_id, &payment, &proof));
}

#[test]
fn test_verify_settlement_proof_returns_false_for_an_invalid_proof() {
    let (env, client, admin) = setup_env();

    let payment_a = make_id(&env, 31);
    let payment_b = make_id(&env, 41);
    let (root, _) = two_leaf_tree(&env, &payment_a, &payment_b);
    let batch_id = client.submit_merkle_root(&admin, &root);

    let bogus = vec![
        &env,
        MerkleProofNode {
            sibling: hash_leaf(&env, &make_id(&env, 99)),
            is_left: false,
        },
    ];

    assert!(!client.verify_settlement_proof(&batch_id, &payment_a, &bogus));
}

#[test]
#[should_panic(expected = "No reconciliation batch with that ID")]
fn test_verify_settlement_proof_panics_for_an_unknown_batch() {
    let (env, client, admin) = setup_env();

    let payment_a = make_id(&env, 50);
    let payment_b = make_id(&env, 60);
    let (root, proof) = two_leaf_tree(&env, &payment_a, &payment_b);
    client.submit_merkle_root(&admin, &root);

    // A missing batch is not the same as an invalid proof: returning false
    // here would let a caller read "this root is gone" as "not settled".
    client.verify_settlement_proof(&7, &payment_a, &proof);
}

#[test]
fn test_get_latest_stored_batch_reports_the_newest_id() {
    let (env, client, admin) = setup_env();

    assert!(client.get_latest_stored_batch().is_none());

    client.submit_merkle_root(&admin, &make_id(&env, 1));
    let newest = make_id(&env, 2);
    client.submit_merkle_root(&admin, &newest);

    let stored = client.get_latest_stored_batch().unwrap();
    assert_eq!(stored.batch_id, 1);
    assert_eq!(stored.batch.merkle_root, newest);
}

#[test]
fn test_get_batch_returns_none_for_an_unknown_id() {
    let (_env, client, _admin) = setup_env();
    assert!(client.get_batch(&0).is_none());
}

#[test]
fn test_current_batch_still_tracks_the_latest_submission() {
    let (env, client, admin) = setup_env();

    client.submit_merkle_root(&admin, &make_id(&env, 1));
    let newest = make_id(&env, 2);
    client.submit_merkle_root(&admin, &newest);

    // Existing callers of get_current_batch see no behaviour change.
    assert_eq!(client.get_current_batch().unwrap().merkle_root, newest);
}
