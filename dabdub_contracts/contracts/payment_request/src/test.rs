#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env, String};

#[test]
fn test_payment_lifecycle_success() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PaymentRequestContract);
    let client = PaymentRequestContractClient::new(&env, &contract_id);

    let merchant = Address::generate(&env);
    let asset = Address::generate(&env);
    let payment_id = String::from_str(&env, "pay_123");
    let amount = 1000i128;
    let expiry = 1000u64;

    // Start ledger at t=0
    env.ledger().set_timestamp(0);

    // 1. Create Payment
    client.create_payment(&payment_id, &merchant, &amount, &asset, &expiry);
    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Pending);
    assert_eq!(payment.amount, amount);
    assert_eq!(payment.id, payment_id);
    assert_eq!(payment.merchant, merchant);
    assert_eq!(payment.asset, asset);
    assert_eq!(payment.created_at, 0);
    assert_eq!(payment.expiry, expiry);

    // 2. Confirm Payment
    client.confirm(&payment_id, &merchant);
    assert_eq!(client.get_payment(&payment_id).status, PaymentStatus::Confirmed);

    // 3. Set Settling
    client.set_settling(&payment_id, &merchant);
    assert_eq!(client.get_payment(&payment_id).status, PaymentStatus::Settling);

    // 4. Settle
    client.settle(&payment_id, &merchant);
    assert_eq!(client.get_payment(&payment_id).status, PaymentStatus::Settled);
}

#[test]
#[should_panic(expected = "Invalid transition: can only settle from Settling")]
fn test_direct_settlement_from_confirmed() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentRequestContract);
    let client = PaymentRequestContractClient::new(&env, &contract_id);

    let merchant = Address::generate(&env);
    let asset = Address::generate(&env);
    let payment_id = String::from_str(&env, "pay_123");
    
    client.create_payment(&payment_id, &merchant, &100, &asset, &1000);
    client.confirm(&payment_id, &merchant);
    
    // Skipping the settling state is rejected
    client.settle(&payment_id, &merchant);
}

#[test]
#[should_panic(expected = "Invalid transition: can only settle from Settling")]
fn test_invalid_settle_from_pending() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentRequestContract);
    let client = PaymentRequestContractClient::new(&env, &contract_id);

    let merchant = Address::generate(&env);
    let asset = Address::generate(&env);
    let payment_id = String::from_str(&env, "pay_123");
    
    client.create_payment(&payment_id, &merchant, &100, &asset, &1000);
    
    // Try to settle directly from pending
    client.settle(&payment_id, &merchant);
}

#[test]
fn test_payment_expiry() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentRequestContract);
    let client = PaymentRequestContractClient::new(&env, &contract_id);

    let merchant = Address::generate(&env);
    let asset = Address::generate(&env);
    let payment_id = String::from_str(&env, "pay_123");
    let expiry = 100u64;

    env.ledger().set_timestamp(0);
    client.create_payment(&payment_id, &merchant, &100, &asset, &expiry);

    // Advance time past expiry
    env.ledger().set_timestamp(101);

    client.expire(&payment_id, &merchant);
    assert_eq!(client.get_payment(&payment_id).status, PaymentStatus::Expired);
}

#[test]
fn test_duplicate_payment_id_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentRequestContract);
    let client = PaymentRequestContractClient::new(&env, &contract_id);

    let merchant = Address::generate(&env);
    let asset = Address::generate(&env);
    let payment_id = String::from_str(&env, "duplicate");

    client.create_payment(&payment_id, &merchant, &100, &asset, &1000);
    let result = client.try_create_payment(&payment_id, &merchant, &200, &asset, &1000);

    assert!(result.is_err());
    assert_eq!(client.get_payment(&payment_id).amount, 100);
}

#[test]
fn test_expired_payment_cannot_be_confirmed() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentRequestContract);
    let client = PaymentRequestContractClient::new(&env, &contract_id);

    let merchant = Address::generate(&env);
    let asset = Address::generate(&env);
    let payment_id = String::from_str(&env, "expired");
    env.ledger().set_timestamp(10);
    client.create_payment(&payment_id, &merchant, &100, &asset, &20);

    env.ledger().set_timestamp(21);
    let result = client.try_confirm(&payment_id);

    assert!(result.is_err());
    assert_eq!(client.get_payment(&payment_id).status, PaymentStatus::Pending);
}

#[test]
fn test_cancelled_payment_cannot_be_confirmed() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentRequestContract);
    let client = PaymentRequestContractClient::new(&env, &contract_id);

    let merchant = Address::generate(&env);
    let asset = Address::generate(&env);
    let payment_id = String::from_str(&env, "cancelled");
    client.create_payment(&payment_id, &merchant, &100, &asset, &1000);
    client.cancel(&payment_id, &merchant);

    assert_eq!(client.get_payment(&payment_id).status, PaymentStatus::Cancelled);
    assert!(client.try_confirm(&payment_id).is_err());
    assert!(client.try_fail(&payment_id).is_err());
}

#[test]
fn test_only_merchant_can_cancel_payment_request() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentRequestContract);
    let client = PaymentRequestContractClient::new(&env, &contract_id);

    let merchant = Address::generate(&env);
    let another_address = Address::generate(&env);
    let asset = Address::generate(&env);
    let payment_id = String::from_str(&env, "merchant-cancel");
    client.create_payment(&payment_id, &merchant, &100, &asset, &1000);

    assert!(client.try_cancel(&payment_id, &another_address).is_err());
    assert_eq!(client.get_payment(&payment_id).status, PaymentStatus::Pending);
}

#[test]
#[should_panic(expected = "Payment has not yet reached expiry time")]
fn test_early_expiry_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentRequestContract);
    let client = PaymentRequestContractClient::new(&env, &contract_id);

    let merchant = Address::generate(&env);
    let asset = Address::generate(&env);
    let payment_id = String::from_str(&env, "pay_123");
    let expiry = 100u64;

    env.ledger().set_timestamp(0);
    client.create_payment(&payment_id, &merchant, &100, &asset, &expiry);

    // Try to expire at t=50
    env.ledger().set_timestamp(50);
    client.expire(&payment_id, &merchant);
}

#[test]
fn test_payment_failure() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentRequestContract);
    let client = PaymentRequestContractClient::new(&env, &contract_id);

    let merchant = Address::generate(&env);
    let asset = Address::generate(&env);
    let payment_id = String::from_str(&env, "pay_123");

    client.create_payment(&payment_id, &merchant, &100, &asset, &1000);
    client.confirm(&payment_id, &merchant);
    
    client.fail(&payment_id, &merchant);
    assert_eq!(client.get_payment(&payment_id).status, PaymentStatus::Failed);
}

#[test]
#[should_panic(expected = "Cannot fail a finalized payment")]
fn test_fail_after_settled_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentRequestContract);
    let client = PaymentRequestContractClient::new(&env, &contract_id);

    let merchant = Address::generate(&env);
    let asset = Address::generate(&env);
    let payment_id = String::from_str(&env, "pay_123");

    client.create_payment(&payment_id, &merchant, &100, &asset, &1000);
    client.confirm(&payment_id, &merchant);
    client.set_settling(&payment_id, &merchant);
    client.settle(&payment_id, &merchant);
    
    client.fail(&payment_id, &merchant);
}

#[test]
#[should_panic(expected = "payment id must not be empty")]
fn test_empty_payment_id_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentRequestContract);
    let client = PaymentRequestContractClient::new(&env, &contract_id);

    let merchant = Address::generate(&env);
    let asset = Address::generate(&env);
    client.create_payment(&String::from_str(&env, ""), &merchant, &100, &asset, &1000);
}

/// Creates a payment with mocked auth, then clears all auths so subsequent
/// calls behave like an unrelated third-party caller.
fn setup_unauthorized(env: &Env) -> (PaymentRequestContractClient<'_>, String, Address) {
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentRequestContract);
    let client = PaymentRequestContractClient::new(env, &contract_id);

    let merchant = Address::generate(env);
    let asset = Address::generate(env);
    let payment_id = String::from_str(env, "pay_auth");
    env.ledger().set_timestamp(0);
    client.create_payment(&payment_id, &merchant, &100, &asset, &1000);
    env.set_auths(&[]);
    (client, payment_id, merchant)
}

#[test]
fn test_unauthorized_confirm_rejected() {
    let env = Env::default();
    let (client, id, merchant) = setup_unauthorized(&env);
    assert!(client.try_confirm(&id, &merchant).is_err());
    assert_eq!(client.get_payment(&id).status, PaymentStatus::Pending);
}

#[test]
fn test_unauthorized_set_settling_rejected() {
    let env = Env::default();
    let (client, id, merchant) = setup_unauthorized(&env);
    env.mock_all_auths();
    client.confirm(&id, &merchant);
    env.set_auths(&[]);
    assert!(client.try_set_settling(&id, &merchant).is_err());
    assert_eq!(client.get_payment(&id).status, PaymentStatus::Confirmed);
}

#[test]
fn test_unauthorized_settle_rejected() {
    let env = Env::default();
    let (client, id, merchant) = setup_unauthorized(&env);
    env.mock_all_auths();
    client.confirm(&id, &merchant);
    client.set_settling(&id, &merchant);
    env.set_auths(&[]);
    assert!(client.try_settle(&id, &merchant).is_err());
    assert_eq!(client.get_payment(&id).status, PaymentStatus::Settling);
}

#[test]
fn test_unauthorized_fail_rejected() {
    let env = Env::default();
    let (client, id, merchant) = setup_unauthorized(&env);
    assert!(client.try_fail(&id, &merchant).is_err());
    assert_eq!(client.get_payment(&id).status, PaymentStatus::Pending);
}

#[test]
fn test_unauthorized_expire_rejected() {
    let env = Env::default();
    let (client, id, merchant) = setup_unauthorized(&env);
    env.ledger().set_timestamp(2000);
    assert!(client.try_expire(&id, &merchant).is_err());
    assert_eq!(client.get_payment(&id).status, PaymentStatus::Pending);
}
