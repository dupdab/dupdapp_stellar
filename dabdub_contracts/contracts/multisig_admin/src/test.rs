#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, vec, Address, Bytes, Env, String};

fn setup_env() -> (
    Env,
    MultisigAdminContractClient<'static>,
    Address,
    Address,
    Address,
) {
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
// Signer set changes during an active operation
//
// Documented behavior: quorum is evaluated against the *current* signer set
// and threshold at the moment an approval is recorded. Approvals already
// counted are retained even if the approving signer is later removed, but a
// removed signer can no longer approve. Adding a signer mid-operation does not
// retroactively change the threshold; the new signer may approve and counts
// toward the existing threshold.
// ---------------------------------------------------------------------------

#[test]
fn test_removed_signer_approval_still_counts_toward_quorum() {
    let (env, client, admin1, admin2, admin3) = setup_env();

    // admin1 proposes (counts as first approval), admin2 approves -> quorum.
    let id = client.propose(&admin1, &op(&env, "transfer"), &args(&env));
    client.approve(&admin2, &id);

    let proposal = client.get_proposal(&id).unwrap();
    assert_eq!(proposal.approvals.len(), 2);
    assert!(proposal.executed);

    // Removing admin2 after the fact does not undo the executed operation.
    let remove = client.propose_admin_change(&admin1, &admin2, &false);
    client.approve(&admin3, &remove);
    assert!(!client.is_admin(&admin2));

    let proposal_after = client.get_proposal(&id).unwrap();
    assert!(proposal_after.executed);
    assert_eq!(proposal_after.approvals.len(), 2);
}

#[test]
fn test_removed_signer_cannot_approve_pending_operation() {
    let (env, client, admin1, admin2, admin3) = setup_env();

    // Raise threshold to 3 so the operation stays pending after one approval.
    let raise = client.propose_threshold_change(&admin1, &3);
    client.approve(&admin2, &raise);
    assert_eq!(client.get_threshold(), 3);

    let id = client.propose(&admin1, &op(&env, "transfer"), &args(&env));
    let pending = client.get_proposal(&id).unwrap();
    assert_eq!(pending.approvals.len(), 1);
    assert!(!pending.executed);

    // Remove admin2 while the operation is still waiting for signatures.
    let remove = client.propose_admin_change(&admin1, &admin2, &false);
    client.approve(&admin3, &remove);
    assert!(!client.is_admin(&admin2));

    // The pending operation is unaffected and still needs more approvals.
    let still_pending = client.get_proposal(&id).unwrap();
    assert_eq!(still_pending.approvals.len(), 1);
    assert!(!still_pending.executed);
}

#[test]
fn test_added_signer_mid_operation_uses_existing_threshold() {
    let (env, client, admin1, admin2, _admin3) = setup_env();
    let new_admin = Address::generate(&env);

    // Start an operation that is waiting for signatures.
    let id = client.propose(&admin1, &op(&env, "transfer"), &args(&env));
    let pending = client.get_proposal(&id).unwrap();
    assert_eq!(pending.approvals.len(), 1);
    assert!(!pending.executed);

    // Add a new signer mid-operation; threshold stays at 2.
    let add = client.propose_admin_change(&admin1, &new_admin, &true);
    client.approve(&admin2, &add);
    assert!(client.is_admin(&new_admin));
    assert_eq!(client.get_threshold(), 2);

    // The new signer can approve and counts toward the existing threshold.
    client.approve(&new_admin, &id);
    let executed = client.get_proposal(&id).unwrap();
    assert_eq!(executed.approvals.len(), 2);
    assert!(executed.executed);
}

// ---------------------------------------------------------------------------
// Generic action dispatch: propose_action / approve_action / execute_action
// ---------------------------------------------------------------------------

fn setup_2_of_3() -> (
    Env,
    MultisigAdminContractClient<'static>,
    Address,
    Address,
    Address,
) {
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

    let id = client.propose_action(&signer1, &op(&env, "transfer"), &target, &args(&env));
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
    assert!(action.executed);
}
