#![cfg(test)]

use crate::{FeeCalculatorContract, FeeCalculatorContractClient, FeeTier, FeeTiersUpdatedEvent};
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
    assert_eq!(bps, 100);
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
    let (env, client, admin, _merchant, _settlement) = setup_env
    assert_eq!(bps_before_reset, 120);

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
    let (env, client, _admin, merchant, _settlement) = setup_env();
    let random = Address::generate(&env);
    client.calculate_fee(&random, &merchant, &500);
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
fn test_non_admin_cannot_set_min_fee() {
    let (env, client, _admin, _merchant) = setup_env();
    let random = Address::generate(&env);

    client.set_min_fee(&random, &5);
}

#[test]
fn test_small_fee_is_clamped_up_to_min_fee() {
    let (_env, client, admin, merchant) = setup_env();

    // With a 1% (100 bps) tier, amount = 1 would truncate to a zero fee.
    client.set_min_fee(&admin, &1);

    let (fee, net, _bps) = client
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

#[test]
fn test_non_admin_cannot_set_min_fee() {
    let (env, client, _admin, _merchant) = setup_env();
    let random = Address::generate(&env);

    client.set_min_fee(&random, &5);
}

#[test]
fn test_small_fee_is_clamped_up_to_min_fee() {
    let (_env, client, admin, merchant) = setup_env();

    // With a 1% (100 bps) tier, amount = 1 would truncate to a zero fee.
    client.set_min_fee(&admin, &1);

    let (fee, net, _bps) = client.calculate_fee(&merchant, &1);
    assert_eq!(fee, 1);
    assert_eq!(net, 0);
}

#[test]
fn test_zero_amount_incurs_no_min_fee() {
    let (_env, client, admin, merchant) = setup_env();

    client.set_min_fee(&admin, &5);

    let (fee, net, _bps) = client.calculate_fee(&merchant, &0);
    assert_eq!(fee, 0);
    assert_eq!(net, 0);
}

#[test]
fn test_fee_above_min_fee_is_not_clamped() {
    let (_env, client, admin, merchant) = setup_env();

    client.set_min_fee(&admin, &1);

    // 150 bps of 10_000 = 150, well above the minimum floor.
    let (fee, net, _bps) = client.calculate_fee(&merchant, &10_000);
    assert_eq!(fee, 150);
    assert_eq!(net, 9_850);
}

/// Documents the current (buggy) behavior: `calculate_fee` has no access
/// control, so an unrelated caller can pass any `merchant` address and still
/// mutate that merchant's tracked volume. This test pins the buggy behavior so
/// that once access control is added, it can be replaced with
/// `test_calculate_fee_unauthorized_caller_panics`.
#[test]
fn test_calculate_fee_from_unrelated_caller_still_mutates_target_merchant_volume() {
    let (env, client, _admin, merchant) = setup_env();
    let attacker = Address::generate(&env);

    // The attacker is not the merchant, yet the call succeeds and mutates the
    // target merchant's volume.
    let (_, _, bps) = client.calculate_fee(&attacker, &2_000);
    assert_eq!(bps, 120);

    // The target merchant's volume was mutated by the unrelated caller: a
    // subsequent call from the merchant itself observes the elevated tier.
    let (_, _, bps_from_merchant) = client.calculate_fee(&merchant, &1);
    assert_eq!(bps_from_merchant, 120);
}

#[test]
#[should_panic(expected = "Not admin")]

}
