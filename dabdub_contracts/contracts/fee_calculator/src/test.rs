#![cfg(test)]

use crate::{FeeCalculatorContract, FeeCalculatorContractClient, FeeTier};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, Env,
};

fn setup_env() -> (Env, FeeCalculatorContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.min_temp_entry_ttl = 300_000;
        li.min_persistent_entry_ttl = 300_000;
        li.max_entry_ttl = 300_000;
    });

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
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

    let contract_id = env.register(FeeCalculatorContract, (&admin, tiers));
    let client = FeeCalculatorContractClient::new(&env, &contract_id);

    (env, client, admin, merchant)
}

#[test]
fn test_fee_rate_drops_when_volume_crosses_tier_threshold() {
    let (_env, client, _admin, merchant) = setup_env();

    // Volume = 900, still tier 1 (150 bps)
    let (_, _, bps_before) = client.calculate_fee(&merchant, &900);
    assert_eq!(bps_before, 150);

    // Volume = 1000, enters tier 2 (120 bps)
    let (_, _, bps_after) = client.calculate_fee(&merchant, &100);
    assert_eq!(bps_after, 120);
}

#[test]
fn test_highest_tier_applies_at_exact_boundary() {
    let (_env, client, _admin, merchant) = setup_env();

    client.calculate_fee(&merchant, &9_999);
    let (_, _, bps) = client.calculate_fee(&merchant, &1);
    assert_eq!(bps, 100);
}

#[test]
fn test_volume_resets_after_30_days_by_ledger_count() {
    let (env, client, _admin, merchant) = setup_env();

    client.calculate_fee(&merchant, &2_000); // puts merchant into 120 bps tier
    let (_, _, bps_before_reset) = client.calculate_fee(&merchant, &1);
    assert_eq!(bps_before_reset, 120);

    // Move ledger beyond 30-day window (172800 ledgers).
    env.ledger().with_mut(|li| li.sequence_number += 172_800);

    let (_, _, bps_after_reset) = client.calculate_fee(&merchant, &100);
    assert_eq!(bps_after_reset, 150);
}

#[test]
fn test_admin_can_update_fee_tiers() {
    let (env, client, admin, _merchant) = setup_env();

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
    let stored = client.get_fee_tiers();
    assert_eq!(stored, new_tiers);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_non_admin_cannot_update_fee_tiers() {
    let (env, client, _admin, _merchant) = setup_env();
    let random = Address::generate(&env);

    let new_tiers = vec![
        &env,
        FeeTier {
            threshold_usdc: 0,
            fee_bps: 100,
        },
    ];

    client.set_fee_tiers(&random, &new_tiers);
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
