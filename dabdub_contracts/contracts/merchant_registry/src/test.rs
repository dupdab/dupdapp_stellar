#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Env, String};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup() -> (Env, MerchantRegistryContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(MerchantRegistryContract, (&admin,));
    let client = MerchantRegistryContractClient::new(&env, &contract_id);

    (env, client, admin)
}

fn sample_name(env: &Env) -> String {
    String::from_str(env, "Acme Corp")
}

// ---------------------------------------------------------------------------
// register_merchant
// ---------------------------------------------------------------------------

#[test]
fn test_register_merchant_happy_path() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));

    let record = client.get_merchant(&merchant);
    assert_eq!(record.merchant, merchant);
    assert_eq!(record.status, MerchantStatus::Active);
}

#[test]
#[should_panic(expected = "Merchant already registered")]
fn test_register_duplicate_merchant_panics() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.register_merchant(&admin, &merchant, &sample_name(&env));
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_register_unauthorized() {
    let (env, client, _admin) = setup();
    let merchant = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.register_merchant(&attacker, &merchant, &sample_name(&env));
}

// ---------------------------------------------------------------------------
// suspend_merchant / reactivate_merchant
// ---------------------------------------------------------------------------

#[test]
fn test_suspend_merchant() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.suspend_merchant(&admin, &merchant);

    let record = client.get_merchant(&merchant);
    assert_eq!(record.status, MerchantStatus::Suspended);
}

#[test]
fn test_reactivate_merchant() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.suspend_merchant(&admin, &merchant);
    client.reactivate_merchant(&admin, &merchant);

    let record = client.get_merchant(&merchant);
    assert_eq!(record.status, MerchantStatus::Active);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_suspend_unauthorized() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.suspend_merchant(&attacker, &merchant);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_reactivate_unauthorized() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.suspend_merchant(&admin, &merchant);
    client.reactivate_merchant(&attacker, &merchant);
}

#[test]
#[should_panic(expected = "Merchant already suspended")]
fn test_suspend_already_suspended() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.suspend_merchant(&admin, &merchant);
    client.suspend_merchant(&admin, &merchant);
}

#[test]
#[should_panic(expected = "Merchant already active")]
fn test_reactivate_already_active() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    // merchant is Active after registration – reactivating should panic
    client.reactivate_merchant(&admin, &merchant);
}

// ---------------------------------------------------------------------------
// terminate_merchant
// ---------------------------------------------------------------------------

#[test]
fn test_terminate_merchant_from_active() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.terminate_merchant(&admin, &merchant);

    let record = client.get_merchant(&merchant);
    assert_eq!(record.status, MerchantStatus::Terminated);
}

#[test]
fn test_terminate_merchant_from_suspended() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.suspend_merchant(&admin, &merchant);
    client.terminate_merchant(&admin, &merchant);

    let record = client.get_merchant(&merchant);
    assert_eq!(record.status, MerchantStatus::Terminated);
}

#[test]
fn test_is_merchant_active_returns_false_after_termination() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.terminate_merchant(&admin, &merchant);

    assert!(!client.is_merchant_active(&merchant));
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_terminate_unauthorized() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.terminate_merchant(&attacker, &merchant);
}

#[test]
#[should_panic(expected = "Merchant not found")]
fn test_terminate_unknown_merchant() {
    let (env, client, admin) = setup();
    let unknown = Address::generate(&env);

    client.terminate_merchant(&admin, &unknown);
}

#[test]
#[should_panic(expected = "Merchant already terminated")]
fn test_terminate_already_terminated() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.terminate_merchant(&admin, &merchant);
    client.terminate_merchant(&admin, &merchant);
}

#[test]
#[should_panic(expected = "Cannot reactivate terminated merchant")]
fn test_reactivate_after_termination_fails() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.terminate_merchant(&admin, &merchant);
    client.reactivate_merchant(&admin, &merchant);
}

#[test]
#[should_panic(expected = "Merchant is terminated")]
fn test_suspend_after_termination_fails() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.terminate_merchant(&admin, &merchant);
    client.suspend_merchant(&admin, &merchant);
}

// ---------------------------------------------------------------------------
// transfer_admin (two-step: propose_admin + accept_admin)
// ---------------------------------------------------------------------------

#[test]
fn test_propose_admin_does_not_take_effect_until_accepted() {
    let (env, client, admin) = setup();
    let new_admin = Address::generate(&env);

    client.propose_admin(&admin, &new_admin);

    // Current admin remains in control until the pending admin accepts.
    assert_eq!(client.get_admin(), admin);
}

#[test]
fn test_accept_admin_completes_transfer() {
    let (env, client, admin) = setup();
    let new_admin = Address::generate(&env);

    client.propose_admin(&admin, &new_admin);
    client.accept_admin(&new_admin);

    assert_eq!(client.get_admin(), new_admin);
}

#[test]
fn test_propose_admin_can_be_corrected_before_acceptance() {
    let (env, client, admin) = setup();
    let wrong_admin = Address::generate(&env);
    let correct_admin = Address::generate(&env);

    client.propose_admin(&admin, &wrong_admin);
    // Re-propose with the corrected address before anyone accepts.
    client.propose_admin(&admin, &correct_admin);
    client.accept_admin(&correct_admin);

    assert_eq!(client.get_admin(), correct_admin);
}

#[test]
fn test_transfer_admin_updates_merchant_ops() {
    let (env, client, admin) = setup();
    let new_admin = Address::generate(&env);
    let merchant = Address::generate(&env);

    client.propose_admin(&admin, &new_admin);
    client.accept_admin(&new_admin);
    client.register_merchant(&new_admin, &merchant, &sample_name(&env));

    let record = client.get_merchant(&merchant);
    assert_eq!(record.merchant, merchant);
    assert_eq!(record.status, MerchantStatus::Active);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_propose_admin_unauthorized() {
    let (env, client, _admin) = setup();
    let attacker = Address::generate(&env);
    let new_admin = Address::generate(&env);

    client.propose_admin(&attacker, &new_admin);
}

#[test]
#[should_panic(expected = "No pending admin")]
fn test_accept_admin_without_proposal_fails() {
    let (env, client, _admin) = setup();
    let new_admin = Address::generate(&env);

    client.accept_admin(&new_admin);
}

#[test]
#[should_panic(expected = "Not pending admin")]
fn test_accept_admin_wrong_caller_fails() {
    let (env, client, admin) = setup();
    let new_admin = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.propose_admin(&admin, &new_admin);
    client.accept_admin(&attacker);
}
