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

// ---------------------------------------------------------------------------
// Generic action dispatch: propose_action / approve_action / execute_action
// ---------------------------------------------------------------------------

fn setup_2_of_3() -> (Env, MultisigAdminContractClient<'static>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let signer1 = Address::generate(&env);
    let signer2 = Address::generate(&env);
    let signer3 = Address::generate(&env);

    let contract_id = env.register(MultisigAdminContract, (&signer1, &signer2, &signer3));
    let client = MultisigAdminContractClient::new(&env, &contract_id);

    (env, client, signer1, signer2, signer3)
}

fn setup_3_of_5() -> (
    Env,
    MultisigAdminContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let s3 = Address::generate(&env);
    let s4 = Address::generate(&env);
    let s5 = Address::generate(&env);

    let contract_id = env.register(MultisigAdminContract, (&s1, &s2, &s3));
    let client = MultisigAdminContractClient::new(&env, &contract_id);

    // Expand the signer set to 5 and raise the threshold to 3.
    let add4 = client.propose_admin_change(&s1, &s4, &true);
    client.approve(&s2, &add4);
    let add5 = client.propose_admin_change(&s1, &s5, &true);
    client.approve(&s2, &add5);
    let raise = client.propose_threshold_change(&s1, &3);
    client.approve(&s2, &raise);

    (env, client, s1, s2, s3, s4, s5)
}

#[test]
fn test_propose_action_only_signer1() {
    let (env, client, signer1, _signer2, _signer3) = setup_2_of_3();
    let target = Address::generate(&env);

    let id = client.propose_action(
        &signer1,
        &op(&env, "transfer"),
        &target,
        &args(&env),
    );
    assert_eq!(id, 0);

    let action = client.get_action(&id).unwrap();
    assert_eq!(action.action_type, op(&env, "transfer"));
    assert_eq!(action.target_contract, target);
    assert_eq!(action.approvals.len(), 1);
    assert!(!action.executed);
}

#[test]
#[should_panic(expected = "only signer 1 can propose")]
fn test_propose_action_rejects_signer2() {
    let (env, client, _signer1, signer2, _signer3) = setup_2_of_3();
    let target = Address::generate(&env);

    client.propose_action(&signer2, &op(&env, "transfer"), &target, &args(&env));
}

#[test]
#[should_panic(expected = "only signer 1 can propose")]
fn test_propose_action_rejects_non_signer() {
    let (env, client, _signer1, _signer2, _signer3) = setup_2_of_3();
    let outsider = Address::generate(&env);
    let target = Address::generate(&env);

    client.propose_action(&outsider, &op(&env, "transfer"), &target, &args(&env));
}

#[test]
fn test_approve_action_by_signer2() {
    let (env, client, signer1, signer2, _signer3) = setup_2_of_3();
    let target = Address::generate(&env);

    let id = client.propose_action(&signer1, &op(&env, "transfer"), &target, &args(&env));
    client.approve_action(&signer2, &id);

    let action = client.get_action(&id).unwrap();
    assert_eq!(action.approvals.len(), 2);
    assert!(!action.executed);
}

#[test]
#[should_panic(expected = "signer 1 cannot approve")]
fn test_approve_action_rejects_signer1() {
    let (env, client, signer1, _signer2, _signer3) = setup_2_of_3();
    let target = Address::generate(&env);

    let id = client.propose_action(&signer1, &op(&env, "transfer"), &target, &args(&env));
    client.approve_action(&signer1, &id);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_approve_action_rejects_non_signer() {
    let (env, client, signer1, _signer2, _signer3) = setup_2_of_3();
    let outsider = Address::generate(&env);
    let target = Address::generate(&env);

    let id = client.propose_action(&signer1, &op(&env, "transfer"), &target, &args(&env));
    client.approve_action(&outsider, &id);
}

#[test]
#[should_panic(expected = "already approved")]
fn test_approve_action_prevents_double_approval() {
    let (env, client, signer1, signer2, _signer3) = setup_2_of_3();
    let target = Address::generate(&env);

    let id = client.propose_action(&signer1, &op(&env, "transfer"), &target, &args(&env));
    client.approve_action(&signer2, &id);
    client.approve_action(&signer2, &id);
}

#[test]
#[should_panic(expected = "quorum not reached")]
fn test_execute_action_before_quorum_fails() {
    let (env, client, signer1, _signer2, _signer3) = setup_2_of_3();
    let target = Address::generate(&env);

    let id = client.propose_action(&signer1, &op(&env, "transfer"), &target, &args(&env));
    client.execute_action(&signer1, &id);
}

#[test]
fn test_execute_action_after_quorum_2_of_3() {
    let (env, client, signer1, signer2, _signer3) = setup_2_of_3();
    let target = Address::generate(&env);

    let id = client.propose_action(&signer1, &op(&env, "transfer"), &target, &args(&env));
    client.approve_action(&signer2, &id);
    client.execute_action(&signer1, &id);

    let action = client.get_action(&id).unwrap();
    assert!(action.executed);
}

#[test]
#[should_panic(expected = "already executed")]
fn test_execute_action_twice_fails() {
    let (env, client, signer1, signer2, _signer3) = setup_2_of_3();
    let target = Address::generate(&env);

    let id = client.propose_action(&signer1, &op(&env, "transfer"), &target, &args(&env));
    client.approve_action(&signer2, &id);
    client.execute_action(&signer1, &id);
    client.execute_action(&signer1, &id);
}

#[test]
fn test_execute_action_after_quorum_3_of_5() {
    let (env, client, s1, s2, s3, _s4, _s5) = setup_3_of_5();
    let target = Address::generate(&env);

    let id = client.propose_action(&s1, &op(&env, "upgrade"), &target, &args(&env));
    client.approve_action(&s2, &id);

    // Only 2 approvals so far; quorum is 3.
    let action = client.get_action(&id).unwrap();
    assert_eq!(action.approvals.len(), 2);

    client.approve_action(&s3, &id);
    client.execute_action(&s1, &id);

    let action = client.get_action(&id).unwrap();
    assert!(action.executed);
    assert_eq!(action.approvals.len(), 3);
}

#[test]
#[should_panic(expected = "action expired")]
fn test_execute_action_after_ttl_expires() {
    let (env, client, signer1, signer2, _signer3) = setup_2_of_3();
    let target = Address::generate(&env);

    let id = client.propose_action(&signer1, &op(&env, "transfer"), &target, &args(&env));
    client.approve_action(&signer2, &id);

    // Advance ledger past the configured TTL.
    env.ledger().set_sequence_number(env.ledger().sequence() + 100_000);

    client.execute_action(&signer1, &id);
}
