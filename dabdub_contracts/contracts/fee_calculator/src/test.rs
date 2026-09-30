#![cfg(test)]

use crate::{FeeCalculatorContract, FeeCalculatorContractClient, FeeTier};
use soroban_sdk::{testutils::{Address as _, Ledger}, vec, Address, Env};

fn setup_env() -> (
    Env,
    FeeCalculatorContractClient<'static>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.min_temp_entry_ttl = 300_000;
        li.min_persistent_entry_ttl = 300_000;
        li.max_entry_ttl = 300_000;
    });

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

    let contract_id = env.register(FeeCalculatorContract, (&admin, tiers));
    let client = FeeCalculatorContractClient::new(&env, &contract_id);
    client.set_settlement_caller(&admin, &settlement_caller);

    (env, client, admin, merchant, settlement_caller)
}

#[test]
fn test_fee_rate_drops_when_volume_crosses_tier_threshold() {
    let (_env, client, _admin, merchant, settlement_caller) = setup_env();

    // Volume = 900, still tier 1 (150 bps)
    let (_, _, bps_before) = client.calculate_fee(&settlement_caller, &merchant, &900);
    assert_eq!(bps_before, 150);

    // Volume = 1000, enters tier 2 (120 bps)
    let (_, _, bps_after) = client.calculate_fee(&settlement_caller, &merchant, &100);
    assert_eq!(bps_after, 120);
}

#[test]
fn test_highest_tier_applies_at_exact_boundary() {
    let (_env, client, _admin, merchant, settlement_caller) = setup_env();

    client.calculate_fee(&settlement_caller, &merchant, &9_999);
    let (_, _, bps) = client.calculate_fee(&settlement_caller, &merchant, &1);
    assert_eq!(bps, 100);
}

#[test]
fn test_volume_resets_after_30_days_by_ledger_count() {
    let (env, client, _admin, merchant, settlement_caller) = setup_env();

    client.calculate_fee(&settlement_caller, &merchant, &2_000); // puts merchant into 120 bps tier
    let (_, _, bps_before_reset) =
        client.calculate_fee(&settlement_caller, &merchant, &1);
    assert_eq!(bps_before_reset, 120);

    // Move ledger beyond 30-day window (172800 ledgers).
    env.ledger().with_mut(|li| li.sequence_number += 172_800);

    let (_, _, bps_after_reset) =
        client.calculate_fee(&settlement_caller, &merchant, &100);
    assert_eq!(bps_after_reset, 150);
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
    let stored = client.get_fee_tiers();
    assert_eq!(stored, new_tiers);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_non_admin_cannot_update_fee_tiers() {
    let (env, client, _admin, _merchant, _settlement_caller) = setup_env();
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
#[should_panic(expected = "Not authorized")]
fn test_calculate_fee_unauthorized_caller_panics() {
    let (env, client, _admin, merchant, _settlement_caller) = setup_env();
    let random = Address::generate(&env);

    // A third-party address with no admin or settlement-caller role must not
    // be able to advance any merchant's volume.
    client.calculate_fee(&random, &merchant, &100);
}

#[test]
fn test_calculate_fee_random_cannot_inflate_victim_volume() {
    let (env, client, _admin, merchant, settlement_caller) = setup_env();
    let attacker = Address::generate(&env);

    let result = client.try_calculate_fee(&attacker, &merchant, &5_000);
    assert!(result.is_err());

    // Victim volume must be untouched by the rejected call.
    let volume = client.get_merchant_volume(&merchant);
    assert_eq!(volume.volume_usdc, 0);

    // Legitimate caller still works afterwards.
    let (_, _, bps) = client.calculate_fee(&settlement_caller, &merchant, &100);
    assert_eq!(bps, 150);
    let volume = client.get_merchant_volume(&merchant);
    assert_eq!(volume.volume_usdc, 100);
}

#[test]
fn test_calculate_fee_admin_and_settlement_caller_both_authorized() {
    let (_env, client, admin, merchant, settlement_caller) = setup_env();

    let (_, _, bps_admin) = client.calculate_fee(&admin, &merchant, &500);
    assert_eq!(bps_admin, 150);

    let (_, _, bps_settlement) =
        client.calculate_fee(&settlement_caller, &merchant, &500);
    assert_eq!(bps_settlement, 120);

    let volume = client.get_merchant_volume(&merchant);
    assert_eq!(volume.volume_usdc, 1_000);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_set_settlement_caller_requires_admin() {
    let (env, client, _admin, _merchant, _settlement_caller) = setup_env();
    let random = Address::generate(&env);
    let new_caller = Address::generate(&env);

    client.set_settlement_caller(&random, &new_caller);
}

#[test]
fn test_get_settlement_caller_returns_set_address() {
    let (env, client, admin, _merchant, settlement_caller) = setup_env();
    assert_eq!(
        client.get_settlement_caller(),
        Some(settlement_caller.clone())
    );

    let new_caller = Address::generate(&env);
    client.set_settlement_caller(&admin, &new_caller);
    assert_eq!(client.get_settlement_caller(), Some(new_caller));
}
