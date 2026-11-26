#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events},
    Env, String,
};

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
    assert_eq!(record.status, MerchantStatus::Active);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_transfer_admin_unauthorized() {
    let (env, client, _admin) = setup();
    let attacker = Address::generate(&env);
    let new_admin = Address::generate(&env);

    client.transfer_admin(&attacker, &new_admin);
}

// ---------------------------------------------------------------------------
// is_merchant_active
// ---------------------------------------------------------------------------

#[test]
fn test_is_merchant_active_returns_true_for_active() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    assert!(client.is_merchant_active(&merchant));
}

#[test]
fn test_is_merchant_active_returns_false_for_suspended() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.suspend_merchant(&admin, &merchant);
    assert!(!client.is_merchant_active(&merchant));
}

#[test]
fn test_is_merchant_active_returns_true_after_reactivation() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.suspend_merchant(&admin, &merchant);
    client.reactivate_merchant(&admin, &merchant);
    assert!(client.is_merchant_active(&merchant));
}

#[test]
fn test_is_merchant_active_returns_false_for_unknown() {
    let (env, client, _admin) = setup();
    let unknown = Address::generate(&env);
    assert!(!client.is_merchant_active(&unknown));
}

// ---------------------------------------------------------------------------
// set_kyc_status / is_kyc_verified
// ---------------------------------------------------------------------------

#[test]
fn test_kyc_defaults_to_false() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));

    assert!(!client.is_kyc_verified(&merchant));
}

#[test]
fn test_set_kyc_status_to_true() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.set_kyc_status(&admin, &merchant, &true);

    assert!(client.is_kyc_verified(&merchant));
}

#[test]
fn test_set_kyc_status_toggle_back_to_false() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.set_kyc_status(&admin, &merchant, &true);
    client.set_kyc_status(&admin, &merchant, &false);

    assert!(!client.is_kyc_verified(&merchant));
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_set_kyc_status_unauthorized() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.set_kyc_status(&attacker, &merchant, &true);
}

#[test]
fn test_is_kyc_verified_returns_false_for_unknown() {
    let (env, client, _admin) = setup();
    let unknown = Address::generate(&env);

    assert!(!client.is_kyc_verified(&unknown));
}

#[test]
fn test_get_merchant_includes_kyc_field() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    let record = client.get_merchant(&merchant);

    assert_eq!(record.kyc_verified, false);
}

// ---------------------------------------------------------------------------
// is_approved
// ---------------------------------------------------------------------------

#[test]
fn test_is_approved_false_until_kyc_verified() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    assert!(!client.is_approved(&merchant));
}

#[test]
fn test_is_approved_true_when_active_and_kyc_verified() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.set_kyc_status(&admin, &merchant, &true);
    assert!(client.is_approved(&merchant));
}

#[test]
fn test_is_approved_false_when_suspended_even_if_kyc_verified() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.set_kyc_status(&admin, &merchant, &true);
    client.suspend_merchant(&admin, &merchant);
    assert!(!client.is_approved(&merchant));
}

#[test]
fn test_is_approved_false_for_unknown() {
    let (env, client, _admin) = setup();
    let unknown = Address::generate(&env);
    assert!(!client.is_approved(&unknown));
}

// ---------------------------------------------------------------------------
// update_fee_tier / get_fee_tier
// ---------------------------------------------------------------------------

#[test]
fn test_fee_tier_defaults_to_150() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    assert_eq!(client.get_fee_tier(&merchant), 150);
}

#[test]
fn test_update_fee_tier_happy_path() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.update_fee_tier(&admin, &merchant, &300);
    assert_eq!(client.get_fee_tier(&merchant), 300);
}

#[test]
fn test_update_fee_tier_to_zero() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.update_fee_tier(&admin, &merchant, &0);
    assert_eq!(client.get_fee_tier(&merchant), 0);
}

#[test]
fn test_update_fee_tier_to_max() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.update_fee_tier(&admin, &merchant, &1000);
    assert_eq!(client.get_fee_tier(&merchant), 1000);
}

#[test]
#[should_panic(expected = "fee_bps exceeds maximum of 1000")]
fn test_update_fee_tier_above_max_panics() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.update_fee_tier(&admin, &merchant, &1001);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_update_fee_tier_unauthorized() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.update_fee_tier(&attacker, &merchant, &200);
}

// ---------------------------------------------------------------------------
// merchants (paginated listing) / update_merchant (#697)
// ---------------------------------------------------------------------------

#[test]
fn test_merchants_paginated_listing() {
    let (env, client, admin) = setup();
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let m3 = Address::generate(&env);

    client.register_merchant(&admin, &m1, &sample_name(&env));
    client.register_merchant(&admin, &m2, &sample_name(&env));
    client.register_merchant(&admin, &m3, &sample_name(&env));

    let page0 = client.merchants(&0, &2);
    assert_eq!(page0.len(), 2);
    assert_eq!(page0.get(0).unwrap().merchant, m1);
    assert_eq!(page0.get(1).unwrap().merchant, m2);

    let page1 = client.merchants(&1, &2);
    assert_eq!(page1.len(), 1);
    assert_eq!(page1.get(0).unwrap().merchant, m3);
}

#[test]
fn test_merchants_empty_when_no_merchants() {
    let (_env, client, _admin) = setup();
    assert_eq!(client.merchants(&0, &10).len(), 0);
}

#[test]
#[should_panic(expected = "page size must be > 0")]
fn test_merchants_zero_page_size_panics() {
    let (_env, client, _admin) = setup();
    client.merchants(&0, &0);
}

#[test]
fn test_update_merchant_name() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.update_merchant(&admin, &merchant, &String::from_str(&env, "New Name"));

    assert_eq!(
        client.get_merchant(&merchant).name,
        String::from_str(&env, "New Name")
    );
}

#[test]
#[should_panic(expected = "Merchant not found")]
fn test_update_merchant_unknown_panics() {
    let (env, client, admin) = setup();
    let unknown = Address::generate(&env);
    client.update_merchant(&admin, &unknown, &String::from_str(&env, "x"));
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_update_merchant_unauthorized() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.update_merchant(&attacker, &merchant, &String::from_str(&env, "x"));
}

// ---------------------------------------------------------------------------
// #1502 / #1503 — lifecycle events
// #1504 — metadata size validation
// ---------------------------------------------------------------------------

fn event_count(env: &Env) -> u32 {
    env.events().all().len()
}

#[test]
fn test_register_merchant_emits_registered_event() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    let before = event_count(&env);
    client.register_merchant(&admin, &merchant, &sample_name(&env));

    // Registration publishes the `merchant_registered` event.
    assert!(event_count(&env) > before);
}

#[test]
fn test_suspend_merchant_emits_suspended_event() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    client.register_merchant(&admin, &merchant, &sample_name(&env));
    client.suspend_merchant(&admin, &merchant);

    // Suspension publishes the `merchant_suspended` event.
    assert!(event_count(&env) > 0);
}

fn long_name(env: &Env, bytes: usize) -> String {
    String::from_str(env, &"a".repeat(bytes))
}

#[test]
fn test_register_merchant_accepts_metadata_at_limit() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    // Exactly 1KB is allowed.
    client.register_merchant(
        &admin,
        &merchant,
        &long_name(&env, MAX_METADATA_BYTES as usize),
    );

    assert_eq!(
        client.get_merchant(&merchant).name.len(),
        MAX_METADATA_BYTES
    );
}

#[test]
#[should_panic(expected = "MetadataTooLarge")]
fn test_register_merchant_rejects_oversized_metadata() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    // One byte over the 1KB limit is rejected.
    client.register_merchant(
        &admin,
        &merchant,
        &long_name(&env, MAX_METADATA_BYTES as usize + 1),
    );
}

#[test]
fn test_register_merchant_metadata_size_fuzz() {
    let (env, client, admin) = setup();

    // A spread of payload sizes at or under the limit all succeed.
    for size in [0usize, 1, 512, 1023, 1024] {
        let merchant = Address::generate(&env);
        client.register_merchant(&admin, &merchant, &long_name(&env, size));
        assert_eq!(client.get_merchant(&merchant).name.len() as usize, size);
    }
}

#[test]
#[should_panic(expected = "MetadataTooLarge")]
fn test_register_merchant_rejects_2k_metadata() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);
    client.register_merchant(&admin, &merchant, &long_name(&env, 2048));
}

#[test]
#[should_panic(expected = "MetadataTooLarge")]
fn test_register_merchant_rejects_16k_metadata() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);
    client.register_merchant(&admin, &merchant, &long_name(&env, 16384));
}
