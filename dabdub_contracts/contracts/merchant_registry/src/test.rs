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
// merchants() paginated listing
// ---------------------------------------------------------------------------

#[test]
fn test_merchants_listing_still_includes_terminated_merchants() {
    let (env, client, admin) = setup();

    let merchant_a = Address::generate(&env);
    let merchant_b = Address::generate(&env);
    let merchant_c = Address::generate(&env);

    client.register_merchant(&admin, &merchant_a, &sample_name(&env));
    client.register_merchant(&admin, &merchant_b, &sample_name(&env));
    client.register_merchant(&admin, &merchant_c, &sample_name(&env));

    // Terminate the middle merchant; it must remain in the index for now.
    client.terminate_merchant(&admin, &merchant_b);
    assert_eq!(
        client.get_merchant(&merchant_b).status,
        MerchantStatus::Terminated
    );

    // Document current behavior: terminated merchants are NOT pruned from the
    // `Merchants` index and keep appearing in the paginated listing.
    let page = client.merchants(&0, &10);
    assert_eq!(page.len(), 3);
    assert!(page.contains(&merchant_a));
    assert!(page.contains(&merchant_b));
    assert!(page.contains(&merchant_c));

    // Pagination still walks the full index, including the terminated entry.
    let first_page = client.merchants(&0, &2);
    assert_eq!(first_page.len(), 2);
    let second_page = client.merchants(&2, &2);
    assert_eq!(second_page.len(), 1);
    assert_eq!(second_page.get(0).unwrap(), merchant_c);
}

// ---------------------------------------------------------------------------
// transfer_admin
// ---------------------------------------------------------------------------

#[test]
fn test_transfer_admin_happy_path() {
    let (env, client, admin) = setup();
    let new_admin = Address::generate(&env);

    client.transfer_admin(&admin, &new_admin);

    assert_eq!(client.get_admin(), new_admin);
}

#[test]
fn test_transfer_admin_updates_merchant_ops() {
    let (env, client, admin) = setup();
    let new_admin = Address::generate(&env);
    let merchant = Address::generate(&env);

    client.transfer_admin(&admin, &new_admin);
    client.register_merchant(&new_admin, &merchant, &sample_name(&env));

    let record = client.get_merchant(&merchant);
    assert_eq!

/* … truncated 9238 chars — edit only what you need near the top … */
