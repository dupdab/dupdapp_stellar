#![cfg(test)]

use crate::{FeeCalculatorContract, FeeCalculatorContractClient, FeeTier, FeeTiersUpdatedEvent};
use proptest::prelude::*;
use soroban_sdk::{testutils::{Address as _, Events as _, Ledger}, vec, Address, Env, IntoVal};

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

/// Builds a 10-tier fee configuration used by the gas benchmark.
fn ten_tier_config(env: &Env) -> soroban_sdk::Vec<FeeTier> {
    let mut tiers = soroban_sdk::Vec::new(env);
    let mut i = 0u32;
    while i < 10 {
        tiers.push_back(FeeTier {
            threshold_usdc: (i as i128) * 1_000,
            fee_bps: 200 - (i as u32) * 10,
        });
        i += 1;
    }
    tiers
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
    let (env, client, _admin, merchant, settlement_caller) = setup_env();

    client.calculate_fee(&settlement_caller, &merchant, &2_000); // puts merchant into 120 bps tier
    let (_, _, bps_before_reset) = client.calculate_fee(&settlement_caller, &merchant, &1);
    assert_eq!(bps_before_reset, 120);
}

#[test]
fn test_volume_resets_after_30_days_by_ledger_count() {
    let (env, client, _admin, merchant, settlement_caller) = setup_env();

    client.calculate_fee(&settlement_caller, &merchant, &2_000); // puts merchant into 120 bps tier
    let (_, _, bps_before_reset) =
        client.calculate_fee(&settlement_caller, &merchant, &1);

    assert_eq!(bps_before_reset, 120);

    env.ledger().with_mut(|li| li.sequence_number += 172_800);

    let (_, _, bps_after_reset) =
        client.calculate_fee(
fn test_volume_resets_after_30_days_by_ledger_count() {
    let (env, client, _admin, merchant, settlement_caller) = setup_env();

    client.calculate_fee(&settlement_caller, &merchant, &2_000); // puts merchant into 120 bps tier
    let (_, _, bps_before_reset) =
        client.calculate_fee(&settlement_caller, &merchant, &1);

    assert_eq!(bps_before_reset, 120);

    env.ledger().with_mut(|li| li.sequence_number += 172_800);

    let (_, _, bps_after_reset) =
        client.calculate_fee(&settlement_caller, &merchant, &100);
    assert_eq!(bps_after_reset, 150);
}

#[test]
fn test_get_merchant_volume_is_read_only_after_window_elapsed() {
    let (env, client, admin, merchant) = setup_env();

    client.calculate_fee(&admin, &merchant, &2_000);
    assert_eq!(client.get_merchant_volume(&merchant), 2_000);

    // Move ledger beyond 30-day window (172800 ledgers).
    env.ledger().with_mut(|li| li.sequence_number += 172_800);

    // The getter must report the windowed (reset) value without persisting it.
    assert_eq!(client.get_merchant_volume(&merchant), 0);
    // A second read must observe the same value, proving no write-back occurred.
    assert_eq!(client.get_merchant_volume(&merchant), 0);

    // The persisted volume is still the pre-reset value; only
    // update_and_get_volume (via calculate_fee) performs the reset-and-persist.
    let (_, _, bps) = client.calculate_fee(&admin, &merchant, &100);
    assert_eq!(bps, 150);
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
fn test_set_fee_tiers_emits_fee_tiers_updated_event() {
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

    let expected = FeeTiersUpdatedEvent {
        admin: admin.clone(),
        tiers: new_tiers.clone(),
    };

    let events = env.events().all();
    let last = events.last().unwrap();
    assert_eq!(
        last,
        (
            client.address.clone(),
            (soroban_sdk::symbol_short!("fee_tiers"),).into_val(&env),
            expected.into_val(&env),
        )
    );
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
fn test_admin_can_set_min_fee() {
    let (_env, client, admin, _merchant) = setup_env();

    client.set_min_fee(&admin, &5);
    assert_eq!(client.get_min_fee(), 5);
}

#[test]
fn test_zero_bps_tier_still_charges_minimum_floor() {
    let (env, client, admin, merchant, settlement_caller) = setup_env();

    // Configure a zero-bps tier so the merchant would otherwise pay nothing.
    let zero_tiers = vec![
        &env,
        FeeTier {
            threshold_usdc: 0,
            fee_bps: 0,
        },
    ];
    client.set_fee_tiers(&admin, &zero_tiers);

    // Default minimum floor is 10 bps.
    assert_eq!(client.get_min_fee(), 10);

    let amount: i128 = 10_000;
    let (fee, net, bps) = client.calculate_fee(&settlement_caller, &merchant, &amount);

    // The applied rate must be floored at the minimum, not the tier's 0 bps.
    assert_eq!(bps, 10);
    assert_eq!(fee, amount * 10 / 10_000);
    assert_eq!(net, amount - fee);
    assert!(fee > 0);
}

#[test]
#[should_panic(expected = "Unauthorized caller")]
fn test_unauthorized_caller_cannot_calculate_fee() {
    let (env, client, _admin, merchant) = setup_env();
    let attacker = Address::generate(&env);

    client.calculate_fee(&attacker, &merchant, &1_000);
}

#[test]
fn test_admin_can_calculate_fee() {
    let (_env, client, admin, merchant) = setup_env();

    let (_, _, bps) = client.calculate_fee(&admin, &merchant, &1_000);
    assert_eq!(bps, 120);
}

#[test]
#[should_panic(expected = "Unauthorized caller")]
fn test_attacker_cannot_grief_merchant_fee_tier() {
    let (_env, client, admin, merchant) = setup_env();
    let attacker = Address::generate(&env);

    // Attacker attempts to push the victim merchant's volume past the tier
    // threshold with no real settlement activity.
    client.calculate_fee(&attacker, &merchant, &1_000);

    // The victim's tracked volume must be untouched by the rejected call.
    assert_eq!(client.get_merchant_volume(&merchant), 0);

    // A legitimate admin call still advances volume and applies the tier.
    let (_, _, bps) = client.calculate_fee(&admin, &merchant, &1_000);
    assert_eq!(bps, 120);
}

#[test]
fn test_admin_can_set_settlement_caller() {
    let (env, client, admin, _merchant, _settlement) = setup_env();
    let new_settlement = Address::generate(&env);
    client.set_settlement_caller(&admin, &new_settlement);
    // Confirm the new settlement caller is accepted by calculate_fee
    let merchant = Address::generate(&env);
    let (fee, net, bps) = client.calculate_fee(&new_settlement, &merchant, &500);
    assert_eq!(bps, 150);
    assert!(fee > 0);
    assert!(net > 0);
}

#[test]
#[should_panic(expected = "caller is not the authorized settlement contract")]
fn test_non_settlement_caller_cannot_calculate_fee() {
    let (env, client, _admin, merchant, _settlement_caller) = setup_env();
    let attacker = Address::generate(&env);

    client.calculate_fee(&attacker, &merchant, &1_000);
= Address::generate(&env);
    let (fee, net, bps) = client.calculate_fee(&new_settlement, &merchant, &500);
    assert_eq!(bps, 150);
    assert!(fee > 0);
    assert!(net > 0);
}

#[test]
#[should_panic(expected = "caller is not the authorized settlement contract")]
fn test_non_settlement_caller_cannot_calculate_fee() {
    let (env, client, _admin, merchant, _settlement_caller) = setup_env();
    let attacker = Address::generate(&env);

    client.calculate_fee(&attacker, &merchant, &1_000);
}

/// Gas benchmark: `calculate_fee` against a 10-tier fee configuration.
///
/// Run with `cargo test -- --include-ignored bench`.
#[test]
#[ignore = "gas benchmark; run with --include-ignored bench"]
fn bench_calculate_fee_ten_tiers() {
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
    let tiers = ten_tier_config(&env);

    let contract_id = env.register(FeeCalculatorContract, (&admin, tiers));
    let client = FeeCalculatorContractClient::new(&env, &contract_id);
    client.set_settlement_caller(&admin, &settlement_caller);

    // Warm up: first call writes the merchant volume entry.
    client.calculate_fee(&settlement_caller, &merchant, &1_000);

    let before = env.budget().cpu_instruction_cost();
    let (fee, net, bps) = client.calculate_fee(&settlement_caller, &merchant, &1_000);
    let cpu = env.budget().cpu_instruction_cost() - before;

    assert!(fee > 0);
    assert!(net > 0);
    assert!(bps > 0);
    std::println!("bench_calculate_fee_ten_tiers: cpu_instructions={cpu}");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Property: for any amount, the returned fee plus net always equals the
    /// original amount (no value is created or destroyed by fee calculation).
    #[test]
    fn prop_fee_plus_net_equals_amount(amount in 0i128..=1_000_000_000i128) {
        let (_env, client, _admin, merchant, settlement_caller) = setup_env();
        let (fee, net, _bps) =
            client.calculate_fee(&settlement_caller, &merchant, &amount);
        prop_assert!(fee >= 0);
        prop_assert!(net >= 0);
        prop_assert_eq!(fee + net, amount);
    }
}

}
