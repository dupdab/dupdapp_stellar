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

    // 2. Confirm Payment
    client.confirm(&payment_id);
    assert_eq!(client.get_payment(&payment_id).status, PaymentStatus::Confirmed);

    // 3. Set Settling
    client.set_settling(&payment_id);
    assert_eq!(client.get_payment(&payment_id).status, PaymentStatus::Settling);

    // 4. Settle
    client.settle(&payment_id);
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
    client.confirm(&payment_id);
    
    // Skipping the settling state is rejected
    client.settle(&payment_id);
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
    client.settle(&payment_id);
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

    client.expire(&payment_id);
    assert_eq!(client.get_payment(&payment_id).status, PaymentStatus::Expired);
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
    client.expire(&payment_id);
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
    client.confirm(&payment_id);
    
    client.fail(&payment_id);
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
    client.confirm(&payment_id);
    client.set_settling(&payment_id);
    client.settle(&payment_id);
    
    client.fail(&payment_id);
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
fn setup_unauthorized(env: &Env) -> (PaymentRequestContractClient<'_>, String) {
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentRequestContract);
    let client = PaymentRequestContractClient::new(env, &contract_id);

    let merchant = Address::generate(env);
    let asset = Address::generate(env);
    let payment_id = String::from_str(env, "pay_auth");
    env.ledger().set_timestamp(0);
    client.create_payment(&payment_id, &merchant, &100, &asset, &1000);
    env.set_auths(&[]);
    (client, payment_id)
}

#[test]
fn test_unauthorized_confirm_rejected() {
    let env = Env::default();
    let (client, id) = setup_unauthorized(&env);
    assert!(client.try_confirm(&id).is_err());
    assert_eq!(client.get_payment(&id).status, PaymentStatus::Pending);
}

#[test]
fn test_unauthorized_set_settling_rejected() {
    let env = Env::default();
    let (client, id) = setup_unauthorized(&env);
    env.mock_all_auths();
    client.confirm(&id);
    env.set_auths(&[]);
    assert!(client.try_set_settling(&id).is_err());
    assert_eq!(client.get_payment(&id).status, PaymentStatus::Confirmed);
}

#[test]
fn test_unauthorized_settle_rejected() {
    let env = Env::default();
    let (client, id) = setup_unauthorized(&env);
    env.mock_all_auths();
    client.confirm(&id);
    client.set_settling(&id);
    env.set_auths(&[]);
    assert!(client.try_settle(&id).is_err());
    assert_eq!(client.get_payment(&id).status, PaymentStatus::Settling);
}

#[test]
fn test_unauthorized_fail_rejected() {
    let env = Env::default();
    let (client, id) = setup_unauthorized(&env);
    assert!(client.try_fail(&id).is_err());
    assert_eq!(client.get_payment(&id).status, PaymentStatus::Pending);
}

#[test]
fn test_unauthorized_expire_rejected() {
    let env = Env::default();
    let (client, id) = setup_unauthorized(&env);
    env.ledger().set_timestamp(2000);
    assert!(client.try_expire(&id).is_err());
    assert_eq!(client.get_payment(&id).status, PaymentStatus::Pending);
}
