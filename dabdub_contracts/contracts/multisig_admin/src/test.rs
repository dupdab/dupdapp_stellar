#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, vec, Address, Bytes, Env, String};

fn setup_env() -> (Env, MultisigAdminContractClient<'static>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let contract_id = env.register(MultisigAdminContract, (&admin1, &admin2, &admin3));
    let client = MultisigAdminContractClient::new(&env, &contract_id);

    (env, client, admin1, admin2, admin3)
}

fn op(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

fn args(env: &Env) -> Bytes {
    Bytes::from_slice(env, b"payload")
}

#[test]
fn test_propose_and_approve_by_admin1_and_admin2() {
    let (env, client, admin1, admin2, _admin3) = setup_env();

    let id = client.propose(&admin1, &op(&env, "transfer"), &args(&env));
    assert_eq!(id, 0);

    let proposal = client.get_proposal(&id).unwrap();
    assert_eq!(proposal.approvals.len(), 1);
    assert!(!proposal.executed);

    client.approve(&admin2, &id);

    let proposal = client.get_proposal(&id).unwrap();
    assert_eq!(proposal.approvals.len(), 2);
    assert!(proposal.executed);
}

#[test]
fn test_admin3_proposes_and_admin1_approves() {
    let (env, client, admin1, _admin2, admin3) = setup_env();

    let id = client.propose(&admin3, &op(&env, "transfer"), &args(&env));
    assert_eq!(id, 0);

    let proposal = client.get_proposal(&id).unwrap();
    assert_eq!(proposal.proposer, admin3);
    assert_eq!(proposal.approvals.len(), 1);
    assert!(!proposal.executed);

    client.approve(&admin1, &id);

    let proposal = client.get_proposal(&id).unwrap();
    assert_eq!(proposal.approvals.len(), 2);
    assert!(proposal.executed);
}

#[test]
fn test_admin1_proposes_and_admin3_approves() {
    let (env, client, admin1, _admin2, admin3) = setup_env();

    let id = client.propose(&admin1, &op(&env, "transfer"), &args(&env));
    assert_eq!(id, 0);

    client.approve(&admin3, &id);

    let proposal = client.get_proposal(&id).unwrap();
    assert_eq!(proposal.approvals.len(), 2);
    assert!(proposal.executed);
}

#[test]
#[should_panic(expected = "proposal already executed")]
fn test_admin3_cannot_approve_executed_proposal() {
    let (env, client, admin1, admin2, admin3) = setup_env();

    let id = client.propose(&admin1, &op(&env, "transfer"), &args(&env));
    client.approve(&admin2, &id);

    let proposal = client.get_proposal(&id).unwrap();
    assert!(proposal.executed);

    client.approve(&admin3, &id);
}

#[test]
#[should_panic(expected = "proposal already executed")]
fn test_admin1_cannot_approve_executed_proposal() {
    let (env, client, admin1, admin2, admin3) = setup_env();

    let id = client.propose(&admin3, &op(&env, "transfer"), &args(&env));
    client.approve(&admin2, &id);

    let proposal = client.get_proposal(&id).unwrap();
    assert!(proposal.executed);

    client.approve(&admin1, &id);
}

#[test]
fn test_any_two_of_three_admins_reach_threshold() {
    let (env, client, admin1, admin2, admin3) = setup_env();

    let id = client.propose(&admin2, &op(&env, "transfer"), &args(&env));
    client.approve(&admin3, &id);

    let proposal = client.get_proposal(&id).unwrap();
    assert_eq!(proposal.approvals.len(), 2);
    assert!(proposal.executed);

    let admins = client.get_admins();
    assert_eq!(admins, vec![&env, admin1, admin2, admin3]);
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
