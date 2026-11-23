#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Events, Address, Env, String};

fn make_payment(env: &Env, amount: i128, memo: &str) -> PaymentInput {
    PaymentInput {
        amount,
        memo: String::from_str(env, memo),
        customer: None,
    }
}

fn deploy_and_init(
    env: &Env,
    min_amount: i128,
    max_amount: i128,
) -> (BatchPaymentContractClient<'_>, Address) {
    let contract_id = env.register(BatchPaymentContract, ());
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
#[should_panic(expected = "merchant exceeded maximum batches per ledger")]
fn test_sixth_batch_for_merchant_in_same_ledger_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);

    let merchant = Address::generate(&env);
    let payments = soroban_sdk::vec![&env, make_payment(&env, 1000, "order")];
    for _ in 0..6 {
        client.create_batch(&merchant, &payments);
    }
}

#[test]
fn test_merchant_batch_limit_resets_on_next_ledger() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);

    let merchant = Address::generate(&env);
    let payments = soroban_sdk::vec![&env, make_payment(&env, 1000, "order")];
    for _ in 0..5 {
        client.create_batch(&merchant, &payments);
    }

    env.ledger().set_sequence_number(env.ledger().sequence() + 1);
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
fn test_batch_over_hard_limit_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);

    let merchant = Address::generate(&env);
    let mut payments = soroban_sdk::vec![&env];
    for i in 0..101_u32 {
        payments.push_back(make_payment(&env, (i as i128 + 1) * 10, "over-limit"));
    }

    let err = client.try_create_batch(&merchant, &payments).unwrap_err().unwrap();
    assert_eq!(err, ContractError::BatchTooLarge);
}

#[test]
fn test_amount_below_min_reverts_entire_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);

    let merchant = Address::generate(&env);
    let payments = soroban_sdk::vec![
        &env,
        make_payment(&env, 500, "valid"),
        PaymentInput {
            amount: 0,
            memo: String::from_str(&env, "invalid"),
            customer: None,
        },
    ];

    let err = client.try_create_batch(&merchant, &payments).unwrap_err().unwrap();
    assert_eq!(err, ContractError::PaymentAmountOutsideLimits);
}

#[test]
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

    let err = client.try_create_batch(&merchant, &payments).unwrap_err().unwrap();
    assert_eq!(err, ContractError::MemoEmpty);
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
fn test_max_batch_size_defaults_to_100() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);
    assert_eq!(client.max_batch_size(), 100);
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

    assert_eq!(client.max_batch_size(), 100);
    client.set_max_batch_size(&10);
    assert_eq!(client.max_batch_size(), 10);
    assert!(client.try_set_max_batch_size(&101).is_err());
    assert_eq!(client.max_batch_size(), 10);
}

#[test]
fn test_amount_min_minus_1_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 100, 1_000);
    let merchant = Address::generate(&env);
    let payments = soroban_sdk::vec![&env, make_payment(&env, 99, "below-min")];
    let err = client.try_create_batch(&merchant, &payments).unwrap_err().unwrap();
    assert_eq!(err, ContractError::PaymentAmountOutsideLimits);
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
fn test_rate_limit_defaults_to_100_per_ledger() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);
    assert_eq!(client.rate_limit(), 100);
}

#[test]
fn test_rate_limit_enforced_after_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);

    // Lower the per-ledger limit so the test stays cheap.
    client.set_rate_limit(&3);
    assert_eq!(client.rate_limit(), 3);

    let merchant = Address::generate(&env);
    let payments = soroban_sdk::vec![&env, make_payment(&env, 100, "rate")];

    // First 3 calls within the ledger succeed.
    for _ in 0..3 {
        let ids = client.create_batch(&merchant, &payments);
        assert_eq!(ids.len(), 1);
    }

    // The 4th call in the same ledger is rate limited.
    let err = client.try_create_batch(&merchant, &payments).unwrap_err().unwrap();
    assert_eq!(err, ContractError::RateLimited);
}

#[test]
fn test_rate_limit_resets_on_next_ledger() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);

    client.set_rate_limit(&2);
    let merchant = Address::generate(&env);
    let payments = soroban_sdk::vec![&env, make_payment(&env, 100, "rate")];

    for _ in 0..2 {
        client.create_batch(&merchant, &payments);
    }
    let err = client.try_create_batch(&merchant, &payments).unwrap_err().unwrap();
    assert_eq!(err, ContractError::RateLimited);

    // New ledger resets the per-account counter.
    env.ledger().set_sequence_number(env.ledger().sequence() + 1);
    let ids = client.create_batch(&merchant, &payments);
    assert_eq!(ids.len(), 1);
}

#[test]
fn test_admin_can_set_rate_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = deploy_and_init(&env, 1, 10_000);

    client.set_rate_limit(&50);
    assert_eq!(client.rate_limit(), 50);
    // Zero is rejected to avoid locking out all callers.
    assert!(client.try_set_rate_limit(&0).is_err());
    assert_eq!(client.rate_limit(), 50);
}
