#![cfg(test)]

use crate::{FeeCalculatorContract, FeeCalculatorContractClient, FeeTier};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    vec, Address, Env,
};

fn setup_env() -> (
    Env,
    FeeCalculatorContractClient<'static>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let settlement_caller = Address::generate(&env);
    let tiers = vec![
        &env,
        FeeTier {
            threshold_usdc: 0,
            fee_bps: 150,
        },
        FeeTier {
            threshold_usdc: 1_000,
            fee_bps: 120,
        },
        FeeTier {
            threshold_usdc: 10_000,
            fee_bps: 100,
        },
    ];

    let contract_id = env.register(FeeCalculatorContract, (&admin, tiers, &10u32));
    let client = FeeCalculatorContractClient::new(&env, &contract_id);
    client.set_settlement_caller(&admin, &settlement_caller);

    (env, client, admin, merchant, settlement_caller)
}

#[test]
fn test_fee_rate_drops_when_volume_crosses_tier_threshold() {
    let (_env, client, _admin, merchant, settlement_caller) = setup_env();

    let (_, _, bps_before) = client.calculate_fee(&settlement_caller, &merchant, &900);
    assert_eq!(bps_before, 150);

    let (_, _, bps_after) = client.calculate_fee(&settlement_caller, &merchant, &100);
    assert_eq!(bps_after, 120);
}

#[test]
fn test_highest_tier_applies_at_exact_boundary() {
    let (env, client, _admin, merchant, settlement_caller) = setup_env();

    client.calculate_fee(&settlement_caller, &merchant, &2_000);
    let (_, _, bps_before_reset) = client.calculate_fee(&settlement_caller, &merchant, &1);
    assert_eq!(bps_before_reset, 120);
    assert_eq!(client.get_merchant_volume(&merchant), 2_001);

    env.ledger().with_mut(|li| li.timestamp += 30 * 24 * 60 * 60 + 1);
    assert_eq!(client.get_merchant_volume(&merchant), 0);
}

#[test]
fn test_volume_resets_after_30_days_by_timestamp() {
    let (env, client, _admin, merchant, settlement_caller) = setup_env();

    client.calculate_fee(&settlement_caller, &merchant, &2_000);
    env.ledger().with_mut(|li| li.timestamp += 30 * 24 * 60 * 60 + 1);
    let (_, _, bps_after_reset) = client.calculate_fee(&settlement_caller, &merchant, &100);
    assert_eq!(bps_after_reset, 150);
    assert_eq!(client.get_merchant_volume(&merchant), 100);
}

#[test]
fn test_admin_can_update_fee_tiers() {
    let (env, client, admin, _merchant, _settlement_caller) = setup_env();

    let new_tiers = vec![
        &env,
        FeeTier {
            threshold_usdc: 0,
            fee_bps: 200,
        },
        FeeTier {
            threshold_usdc: 5_000,
            fee_bps: 80,
        },
    ];

    client.set_fee_tiers(&admin, &new_tiers);
    assert_eq!(client.get_fee_tiers(), new_tiers);
}

#[test]
fn test_admin_can_set_min_fee() {
    let (_env, client, admin, _merchant, _settlement_caller) = setup_env();

    client.set_min_fee(&admin, &5);
    assert_eq!(client.get_min_fee(), 5);
}

#[test]
fn test_zero_bps_tier_still_charges_minimum_floor() {
    let (env, client, admin, merchant, settlement_caller) = setup_env();

    let zero_tiers = vec![
        &env,
        FeeTier {
            threshold_usdc: 0,
            fee_bps: 0,
        },
    ];
    client.set_fee_tiers(&admin, &zero_tiers);

    assert_eq!(client.get_min_fee(), 10);

    let amount: i128 = 10_000;
    let (fee, net, bps) = client.calculate_fee(&settlement_caller, &merchant, &amount);
    assert_eq!(bps, 10);
    assert_eq!(fee, amount * 10 / 10_000);
    assert_eq!(net, amount - fee);
    assert!(fee > 0);
}

#[test]
#[should_panic(expected = "Not authorized")]
fn test_unauthorized_caller_cannot_calculate_fee() {
    let (env, client, _admin, merchant, _settlement_caller) = setup_env();
    let attacker = Address::generate(&env);

    client.calculate_fee(&attacker, &merchant, &1_000);
}
