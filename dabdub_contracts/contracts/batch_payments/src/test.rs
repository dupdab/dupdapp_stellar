#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Events, Address, Env, String};

fn make_payment(env: &Env, amount: i128, memo: &str) -> PaymentInput {
    PaymentInput {
        amount,
        memo: String::from_str(env, memo),
        customer: None,
    }
}

fn deploy_and_init(env: &Env, min_amount: i128, max_amount: i128) -> (BatchPaymentContractClient, Address) {
    let contract_id = env.register_contract(None, BatchPaymentContract);
    let client = BatchPaymentContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.init(&admin, &min_amount, &max_amount);
    (client, admin)
}

#[test]
fn test_batch_of_1_payment() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);

    let merchant = Address::generate(&env);
    let payments = soroban_sdk::vec![&env, make_payment(&env, 1000, "order-1")];

    let ids = client.create_batch(&merchant, &payments);
    assert_eq!(ids.len(), 1);
}

#[test]
fn test_batch_of_10_payments() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);

    let merchant = Address::generate(&env);
    let mut payments = soroban_sdk::vec![&env];
    for i in 0..10_u32 {
        payments.push_back(make_payment(&env, (i as i128 + 1) * 100, "order"));
    }

    let ids = client.create_batch(&merchant, &payments);
    assert_eq!(ids.len(), 10);
}

#[test]
fn test_batch_of_20_payments() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);

    let merchant = Address::generate(&env);
    let mut payments = soroban_sdk::vec![&env];
    for i in 0..20_u32 {
        payments.push_back(make_payment(&env, (i as i128 + 1) * 50, "batch-item"));
    }

    let ids = client.create_batch(&merchant, &payments);
    assert_eq!(ids.len(), 20);
}

#[test]
#[should_panic(expected = "batch exceeds maximum")]
fn test_batch_over_20_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);

    let merchant = Address::generate(&env);
    let mut payments = soroban_sdk::vec![&env];
    for i in 0..21_u32 {
        payments.push_back(make_payment(&env, (i as i128 + 1) * 10, "over-limit"));
    }

    client.create_batch(&merchant, &payments);
}

#[test]
#[should_panic(expected = "outside configured limits")]
fn test_amount_below_min_reverts_entire_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);

    let merchant = Address::generate(&env);
    // First item is valid, second has zero amount — entire batch must revert.
    let payments = soroban_sdk::vec![
        &env,
        make_payment(&env, 500, "valid"),
        PaymentInput {
            amount: 0,
            memo: String::from_str(&env, "invalid"),
            customer: None,
        },
    ];

    client.create_batch(&merchant, &payments);
}

#[test]
#[should_panic(expected = "memo must not be empty")]
fn test_empty_memo_reverts_entire_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);

    let merchant = Address::generate(&env);
    let payments = soroban_sdk::vec![
        &env,
        PaymentInput {
            amount: 100,
            memo: String::from_str(&env, ""),
            customer: None,
        },
    ];

    client.create_batch(&merchant, &payments);
}

#[test]
fn test_payment_created_events_emitted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);

    let merchant = Address::generate(&env);
    let payments = soroban_sdk::vec![
        &env,
        make_payment(&env, 100, "item-a"),
        make_payment(&env, 200, "item-b"),
        make_payment(&env, 300, "item-c"),
    ];

    let ids = client.create_batch(&merchant, &payments);
    assert_eq!(ids.len(), 3);

    // Verify three PaymentCreated events were emitted
    let events = env.events().all();
    assert_eq!(events.len(), 3);
}

#[test]
fn test_max_batch_size_is_20() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);
    assert_eq!(client.max_batch_size(), 20);
}

#[test]
fn test_get_payment_returns_record_after_create_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);

    let merchant = Address::generate(&env);
    let payments = soroban_sdk::vec![
        &env,
        make_payment(&env, 500, "order-abc"),
        make_payment(&env, 750, "order-def"),
    ];

    let ids = client.create_batch(&merchant, &payments);
    assert_eq!(ids.len(), 2);

    let id0 = ids.get(0).unwrap();
    let record0 = client.get_payment(&id0).unwrap();
    assert_eq!(record0.id, id0);
    assert_eq!(record0.amount, 500);
    assert_eq!(record0.memo, String::from_str(&env, "order-abc"));
    assert_eq!(record0.merchant, merchant);

    let id1 = ids.get(1).unwrap();
    let record1 = client.get_payment(&id1).unwrap();
    assert_eq!(record1.id, id1);
    assert_eq!(record1.amount, 750);
    assert_eq!(record1.memo, String::from_str(&env, "order-def"));
    assert_eq!(record1.merchant, merchant);
}

#[test]
fn test_admin_can_set_max_batch_size() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);

    assert_eq!(client.max_batch_size(), 20);
    client.set_max_batch_size(&10);
    assert_eq!(client.max_batch_size(), 10);
}

#[test]
#[should_panic(expected = "outside configured limits")]
fn test_amount_min_minus_1_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 100, 1_000);
    let merchant = Address::generate(&env);
    let payments = soroban_sdk::vec![&env, make_payment(&env, 99, "below-min")];
    client.create_batch(&merchant, &payments);
}

#[test]
fn test_amount_equal_min_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 100, 1_000);
    let merchant = Address::generate(&env);
    let payments = soroban_sdk::vec![&env, make_payment(&env, 100, "at-min")];
    let ids = client.create_batch(&merchant, &payments);
    assert_eq!(ids.len(), 1);
}

#[test]
fn test_amount_equal_max_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 100, 1_000);
    let merchant = Address::generate(&env);
    let payments = soroban_sdk::vec![&env, make_payment(&env, 1_000, "at-max")];
    let ids = client.create_batch(&merchant, &payments);
    assert_eq!(ids.len(), 1);
}

#[test]
#[should_panic(expected = "outside configured limits")]
fn test_amount_max_plus_1_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 100, 1_000);
    let merchant = Address::generate(&env);
    let payments = soroban_sdk::vec![&env, make_payment(&env, 1_001, "above-max")];
    client.create_batch(&merchant, &payments);
}

#[test]
fn test_admin_can_update_limits_without_redeploy() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 100, 1_000);

    let original = client.get_limits();
    assert_eq!(original, (100, 1_000));

    client.set_limits(&200, &2_000);
    let updated = client.get_limits();
    assert_eq!(updated, (200, 2_000));
}

#[test]
fn test_limits_updated_event_emitted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 100, 1_000);

    let before = env.events().all().len();
    client.set_limits(&150, &1_500);
    let events = env.events().all();
    assert_eq!(events.len(), before + 1);
}
