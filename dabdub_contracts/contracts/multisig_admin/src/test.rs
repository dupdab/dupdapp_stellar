#![cfg(test)]

use crate::{MultisigAdminContract, MultisigAdminContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Bytes, Env, String,
};

fn setup_env() -> (
    Env,
    MultisigAdminContractClient<'static>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let contract_id = env.register(
        MultisigAdminContract,
        (&admin1, &admin2, &admin3),
    );
    let client = MultisigAdminContractClient::new(&env, &contract_id);

    (env, client, admin1, admin2, admin3)
}

#[test]
fn test_proposal_executes_only_after_second_admin_approval() {
    let (_env, client, admin1, admin2, _admin3) = setup_env();

    let proposal_id = client.propose(
        &admin1,
        &String::from_str(&_env, "fee_change"),
        &Bytes::from_slice(&_env, &[1, 2, 3]),
    );

    let proposal_before = client.get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal_before.approvals.len(), 1);
    assert!(!proposal_before.executed);

    client.approve(&admin2, &proposal_id);
    let proposal_after = client.get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal_after.approvals.len(), 2);
    assert!(proposal_after.executed);
}

#[test]
#[should_panic(expected = "proposal expired")]
fn test_proposal_expires_after_24_hours() {
    let (env, client, admin1, admin2, _admin3) = setup_env();

    let proposal_id = client.propose(
        &admin1,
        &String::from_str(&env, "emergency_drain"),
        &Bytes::from_slice(&env, &[9]),
    );

    env.ledger().set_timestamp(1_700_000_000 + 86_401);
    client.approve(&admin2, &proposal_id);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_non_admin_cannot_propose() {
    let (env, client, _admin1, _admin2, _admin3) = setup_env();
    let outsider = Address::generate(&env);

    client.propose(
        &outsider,
        &String::from_str(&env, "merchant_termination"),
        &Bytes::from_slice(&env, &[7]),
    );
}

#[test]
#[should_panic(expected = "proposer cannot approve twice")]
fn test_proposer_cannot_approve_own_proposal_twice() {
    let (env, client, admin1, _admin2, _admin3) = setup_env();

    let proposal_id = client.propose(
        &admin1,
        &String::from_str(&env, "merchant_termination"),
        &Bytes::from_slice(&env, &[4, 5]),
    );

    client.approve(&admin1, &proposal_id);
}

#[test]
fn test_admin_rotation_requires_threshold_approval() {
    let (env, client, admin1, admin2, _admin3) = setup_env();
    let new_admin = Address::generate(&env);

    let proposal_id = client.propose_admin_change(&admin1, &new_admin, &true);

    let proposal_before = client.get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal_before.approvals.len(), 1);
    assert!(!proposal_before.executed);

    client.approve(&admin2, &proposal_id);

    let proposal_after = client.get_proposal(&proposal_id).unwrap();
    assert!(proposal_after.executed);
    assert!(client.is_admin(&new_admin));
}

#[test]
fn test_threshold_change_requires_threshold_approval() {
    let (env, client, admin1, admin2, _admin3) = setup_env();

    let proposal_id = client.propose_threshold_change(&admin1, &3);

    let proposal_before = client.get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal_before.approvals.len(), 1);
    assert!(!proposal_before.executed);

    client.approve(&admin2, &proposal_id);

    let proposal_after = client.get_proposal(&proposal_id).unwrap();
    assert!(proposal_after.executed);
    assert_eq!(client.get_threshold(), 3);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_non_admin_cannot_propose_admin_change() {
    let (env, client, _admin1, _admin2, _admin3) = setup_env();
    let outsider = Address::generate(&env);
    let new_admin = Address::generate(&env);

    client.propose_admin_change(&outsider, &new_admin, &true);
}
