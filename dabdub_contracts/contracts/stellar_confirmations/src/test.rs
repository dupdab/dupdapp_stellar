#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

use crate::{StellarConfirmationsContract, StellarConfirmationsContractClient};

fn pid(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn setup(required: u32) -> (Env, Address, StellarConfirmationsContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(StellarConfirmationsContract, (&admin, required));
    let client = StellarConfirmationsContractClient::new(&env, &id);
    (env, admin, client)
}

// helper: call confirm_payment n times
fn confirm_n(
    client: &StellarConfirmationsContractClient,
    admin: &Address,
    payment_id: &BytesN<32>,
    merchant: &Address,
    n: u32,
) -> u32 {
    let mut last = 0;
    for i in 0..n {
        last = client.confirm_payment(admin, payment_id, &(1000 + i), &100_000, merchant);
    }
    last
}

#[test]
fn test_not_settling_before_threshold() {
    let (env, admin, client) = setup(3);
    let payment_id = pid(&env, 1);
    let merchant = Address::generate(&env);

    confirm_n(&client, &admin, &payment_id, &merchant, 2);

    assert_eq!(client.get_payment_confirmations(&payment_id), 2);
    assert!(!client.is_settling(&payment_id));
}

#[test]
fn test_settles_exactly_at_threshold() {
    let (env, admin, client) = setup(3);
    let payment_id = pid(&env, 2);
    let merchant = Address::generate(&env);

    let count = confirm_n(&client, &admin, &payment_id, &merchant, 3);

    assert_eq!(count, 3);
    assert!(client.is_settling(&payment_id));
}

#[test]
fn test_threshold_of_one() {
    let (env, admin, client) = setup(1);
    let payment_id = pid(&env, 3);
    let merchant = Address::generate(&env);

    client.confirm_payment(&admin, &payment_id, &1001, &50_000, &merchant);

    assert!(client.is_settling(&payment_id));
}

#[test]
fn test_extra_confirmations_after_threshold_panic() {
    let (env, admin, client) = setup(2);
    let payment_id = pid(&env, 4);
    let merchant = Address::generate(&env);

    confirm_n(&client, &admin, &payment_id, &merchant, 2);
    assert!(client.is_settling(&payment_id));

    // further calls must panic — payment already settling
    let result = client.try_confirm_payment(&admin, &payment_id, &1010, &50_000, &merchant);
    assert!(result.is_err());
}

#[test]
fn test_independent_payments_dont_interfere() {
    let (env, admin, client) = setup(3);
    let p1 = pid(&env, 5);
    let p2 = pid(&env, 6);
    let merchant = Address::generate(&env);

    confirm_n(&client, &admin, &p1, &merchant, 3);
    confirm_n(&client, &admin, &p2, &merchant, 1);

    assert!(client.is_settling(&p1));
    assert!(!client.is_settling(&p2));
    assert_eq!(client.get_payment_confirmations(&p2), 1);
}

#[test]
fn test_update_confirmation_count() {
    let (env, admin, client) = setup(5);
    client.set_confirmation_count(&admin, &2);
    assert_eq!(client.get_confirmation_count(), 2);

    let payment_id = pid(&env, 7);
    let merchant = Address::generate(&env);
    confirm_n(&client, &admin, &payment_id, &merchant, 2);
    assert!(client.is_settling(&payment_id));
}

#[test]
fn test_lowering_confirmation_count_mid_flight_weakens_guarantee() {
    // Documents current (buggy) behavior: lowering ConfirmationCount after a
    // payment has already accrued some confirmations under the higher
    // threshold causes it to settle immediately at the new, lower threshold —
    // the payment does not need to re-accrue confirmations from scratch.
    let (env, admin, client) = setup(5);
    let payment_id = pid(&env, 9);
    let merchant = Address::generate(&env);

    // Record some confirmations under the higher threshold — not enough to settle.
    confirm_n(&client, &admin, &payment_id, &merchant, 2);
    assert!(!client.is_settling(&payment_id));

    // Admin lowers the required confirmation count mid-flight.
    client.set_confirmation_count(&admin, &2);
    assert_eq!(client.get_confirmation_count(), 2);

    // A single further confirmation now settles the payment immediately at
    // the lower, now-current threshold.
    client.confirm_payment(&admin, &payment_id, &2001, &100_000, &merchant);

    assert_eq!(client.get_payment_confirmations(&payment_id), 3);
    assert!(client.is_settling(&payment_id));
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_confirmation_count_unauthorized_caller_panics() {
    let (env, _admin, client) = setup(5);
    let not_admin = Address::generate(&env);

    client.set_confirmation_count(&not_admin, &2);
}

#[test]
fn test_large_threshold() {
    let (env, admin, client) = setup(12);
    let payment_id = pid(&env, 8);
    let merchant = Address::generate(&env);

    confirm_n(&client, &admin, &payment_id, &merchant, 11);
    assert!(!client.is_settling(&payment_id));

    client.confirm_payment(&admin, &payment_id, &2000, &1_000_000, &merchant);
    assert!(client.is_settling(&payment_id));
}
