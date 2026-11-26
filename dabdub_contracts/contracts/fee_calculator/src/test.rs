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

/// Helper: assert fee + net == amount and fee matches the expected bps math
/// with no precision loss for the given amount (already scaled to the token's
/// smallest unit).
fn assert_fee_no_precision_loss(
    client: &FeeCalculatorContractClient,
    settlement_caller: &Address,
    merchant: &Address,
    amount: i128,
    expected_bps: u32,
) {
    let (fee, net, bps) = client.calculate_fee(settlement_caller, merchant, &amount);
    assert_eq!(bps, expected_bps);
    // Exact integer math: fee is floor(amount * bps / 10_000).
    let expected_fee = amount * (expected_bps as i128) / 10_000;
    assert_eq!(fee, expected_fee, "fee mismatch for amount {amount}");
    // No value is created or destroyed.
    assert_eq!(fee + net, amount, "fee + net must equal amount");
    assert!(fee >= 0);
    assert!(net >= 0);
}

#[test]
fn test_fee_calculation_six_decimal_token() {
    let (_env, client, _admin, merchant, settlement_caller) = setup_env();

    // 6-decimal token: 1 unit = 1_000_000 smallest units.
    // 100 tokens at the base 150 bps tier.
    let amount: i128 = 100 * 1_000_000;
    assert_fee_no_precision_loss(&client, &settlement_caller, &merchant, amount, 150);

    // A tiny amount that would round to zero fee at 150 bps.
    let tiny: i128 = 66; // 66 * 150 / 10_000 = 0 (floor)
    assert_fee_no_precision_loss(&client, &settlement_caller, &merchant, tiny, 150);
}

#[test]
fn test_fee_calculation_seven_decimal_token_usdc() {
    let (_env, client, _admin, merchant, settlement_caller) = setup_env();

    // USDC on Stellar has 7 decimals: 1 unit = 10_000_000 smallest units.
    // 100 USDC at the base 150 bps tier.
    let amount: i128 = 100 * 10_000_000;
    assert_fee_no_precision_loss(&client, &settlement_caller, &merchant, amount, 150);

    // 1 USDC (10_000_000 stroops) at 150 bps = 150_000 stroops exactly.
    let one_usdc: i128 = 10_000_000;
    let (fee, net, bps) = client.calculate_fee(&settlement_caller, &merchant, &one_usdc);
    assert_eq!(bps, 150);
    assert_eq!(fee, 150_000);
    assert_eq!(net, one_usdc - 150_000);
    assert_eq!(fee + net, one_usdc);
}

#[test]
fn test_fee_calculation_zero_decimal_token() {
    let (_env, client, _admin, merchant, settlement_caller) = setup_env();

    // 0-decimal token: 1 unit = 1 smallest unit. Amounts are whole units.
    let amount: i128 = 1_000;
    assert_fee_no_precision_loss(&client, &settlement_caller, &merchant, amount, 150);

    // Smallest possible non-zero amount: 1 unit at 150 bps floors to 0 fee.
    let one: i128 = 1;
    let (fee, net, bps) = client.calculate_fee(&settlement_caller, &merchant, &one);
    assert_eq!(bps, 150);
    assert_eq!(fee, 0);
    assert_eq!(net, 1);
    assert_eq!(fee + net, one);
}

#[test]
fn test_fee_calculation_no_precision_loss_across_decimals() {
    let (_env, client, _admin, merchant, settlement_caller) = setup_env();

    // Same nominal amount expressed in each token's smallest unit.
    // 0-decimal: 1_000 units; 6-decimal: 1_000 * 10^6; 7-decimal: 1_000 * 10^7.
    let amounts: [i128; 3] = [1_000, 1_000 * 1_000_000, 1_000 * 10_000_000];

    for amount in amounts.iter() {
        assert_fee_no_precision_loss(&client, &settlement_caller, &merchant, *amount, 150);
    }
}

#[test]
fn test_fee_rounds_up_to_favor_platform() {
    // 100 USDC at 1.5% (150 bps) = 1.5 USDC, which must round UP to 2 USDC.
    let (_env, client, _admin, merchant) = setup_env();

    let (fee, net, bps) = client.calculate_fee(&merchant, &100);
    assert_eq!(bps, 150);
    assert_eq!(fee, 2);
    assert_eq!(net, 98);
}

#[test]
fn test_fee_ceiling_never_rounds_fractional_cent_down() {
    // 1 USDC at 150 bps = 0.015 USDC -> rounds up to 1 (never 0).
    let (_env, client, _admin, merchant) = setup_env();

    let (fee, _net, _bps) = client.calculate_fee(&merchant, &1);
    assert_eq!(fee, 1);
}
