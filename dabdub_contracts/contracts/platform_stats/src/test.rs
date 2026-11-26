#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Ledger, Env};

fn setup() -> (Env, PlatformStatsContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    // Extend entry TTLs so storage survives the large ledger jumps some tests
    // make (e.g. the 24h bucket rollover), mirroring fee_calculator's setup.
    env.ledger().with_mut(|li| {
        li.min_temp_entry_ttl = 300_000;
        li.min_persistent_entry_ttl = 300_000;
        li.max_entry_ttl = 300_000;
    });

    let admin = Address::generate(&env);
    let id = env.register(PlatformStatsContract, (&admin,));
    let client = PlatformStatsContractClient::new(&env, &id);
    (env, client, admin)
}

#[test]
fn test_initial_stats_are_zero() {
    let (env, client, _admin) = setup();
    let stats = client.stats();
    assert_eq!(stats.total_merchants, 0);
    assert_eq!(stats.total_payments, 0);
    assert_eq!(stats.total_settled_volume_usd, 0);
    assert_eq!(stats.active_payments_24h, 0);
    assert!(stats.health.storage_ok);
    assert!(stats.health.stellar_ok);
    assert!(stats.health.partner_ok);
}

#[test]
fn test_record_merchant_and_payment() {
    let (_env, client, admin) = setup();
    client.record_merchant(&admin);
    client.record_payment(&admin, &10_000_000i128, &true);
    client.record_payment(&admin, &5_000_000i128, &true);
    client.record_payment(&admin, &7_000_000i128, &false);

    let stats = client.stats();
    assert_eq!(stats.total_merchants, 1);
    assert_eq!(stats.total_payments, 3);
    assert_eq!(stats.total_settled_volume_usd, 15_000_000);
    assert_eq!(stats.active_payments_24h, 3);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_record_merchant_requires_admin() {
    let (env, client, _admin) = setup();
    let attacker = Address::generate(&env);
    client.record_merchant(&attacker);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_record_payment_requires_admin() {
    let (env, client, _admin) = setup();
    let attacker = Address::generate(&env);
    client.record_payment(&attacker, &100i128, &true);
}

#[test]
fn test_record_merchant_terminated_decrements_total_merchants() {
    let (_env, client, admin) = setup();
    client.record_merchant(&admin);
    client.record_merchant(&admin);
    assert_eq!(client.stats().total_merchants, 2);

    client.record_merchant_terminated(&admin);
    assert_eq!(client.stats().total_merchants, 1);

    client.record_merchant_terminated(&admin);
    assert_eq!(client.stats().total_merchants, 0);
}

#[test]
fn test_record_merchant_terminated_saturates_at_zero() {
    let (_env, client, admin) = setup();
    // No merchants recorded yet; termination must not underflow.
    client.record_merchant_terminated(&admin);
    assert_eq!(client.stats().total_merchants, 0);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_record_merchant_terminated_requires_admin() {
    let (env, client, _admin) = setup();
    let attacker = Address::generate(&env);
    client.record_merchant_terminated(&attacker);
}

#[test]
fn test_record_settlement_reversed_decrements_volume() {
    let (_env, client, admin) = setup();
    client.record_payment(&admin, &10_000_000i128, &true);
    client.record_payment(&admin, &5_000_000i128, &true);
    assert_eq!(client.stats().total_settled_volume_usd, 15_000_000);

    client.record_settlement_reversed(&admin, &5_000_000i128);
    assert_eq!(client.stats().total_settled_volume_usd, 10_000_000);

    // total_payments is a lifetime counter and is not affected by a reversal.
    assert_eq!(client.stats().total_payments, 2);
}

#[test]
fn test_record_settlement_reversed_saturates_at_zero() {
    let (_env, client, admin) = setup();
    client.record_payment(&admin, &1_000_000i128, &true);

    // Reversing more than was ever settled must not underflow.
    client.record_settlement_reversed(&admin, &5_000_000i128);
    assert_eq!(client.stats().total_settled_volume_usd, 0);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_record_settlement_reversed_requires_admin() {
    let (env, client, _admin) = setup();
    let attacker = Address::generate(&env);
    client.record_settlement_reversed(&attacker, &100i128);
}

#[test]
fn test_partner_health_status_toggle() {
    let (env, client, admin) = setup();
    assert!(client.stats().health.partner_ok);
    client.set_partner_ok(&admin, &false);
    assert!(!client.stats().health.partner_ok);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_partner_ok_requires_admin() {
    let (env, client, _admin) = setup();
    let attacker = Address::generate(&env);
    client.set_partner_ok(&attacker, &false);
}

#[test]
fn test_active_payments_24h_resets_after_bucket_rollover() {
    let (env, client, admin) = setup();

    // Record payments in the first 24h bucket.
    client.record_payment(&admin, &1_000_000i128, &true);
    client.record_payment(&admin, &2_000_000i128, &true);
    assert_eq!(client.stats().active_payments_24h, 2);

    // Advance the ledger into the next ACTIVE_WINDOW_LEDGERS bucket. Note the
    // contract counts activity in fixed sequential buckets keyed by
    // `ledger_sequence / ACTIVE_WINDOW_LEDGERS` (not a true rolling 24h
    // window), so the counter reads from a fresh bucket of 0 afterwards.
    env.ledger().set_sequence_number(2 * ACTIVE_WINDOW_LEDGERS);

    assert_eq!(client.stats().active_payments_24h, 0);

    // Payments recorded after the rollover accrue to the new bucket without
    // resurrecting the previous bucket's count.
    client.record_payment(&admin, &3_000_000i128, &true);
    assert_eq!(client.stats().active_payments_24h, 1);

    // Net totals are unaffected by the bucket rollover.
    let stats = client.stats();
    assert_eq!(stats.total_payments, 3);
    assert_eq!(stats.total_settled_volume_usd, 6_000_000);
}

#[test]
fn test_stats_history_snapshot_creation_and_retrieval() {
    let (env, client, admin) = setup();

    // Day 0: record some activity and take a snapshot.
    client.record_merchant(&admin);
    client.record_payment(&admin, &10_000_000i128, &true);
    client.snapshot_stats(&admin);

    let history = client.get_stats_history(&1u32);
    assert_eq!(history.len(), 1);
    let snap = history.get(0).unwrap();
    assert_eq!(snap.total_merchants, 1);
    assert_eq!(snap.total_payments, 1);
    assert_eq!(snap.total_settled_volume_usd, 10_000_000);

    // Advance one day and take another snapshot with more activity.
    env.ledger().set_sequence_number(DAY_LEDGERS);
    client.record_payment(&admin, &5_000_000i128, &true);
    client.snapshot_stats(&admin);

    let history = client.get_stats_history(&2u32);
    assert_eq!(history.len(), 2);
    let latest = history.get(1).unwrap();
    assert_eq!(latest.total_payments, 2);
    assert_eq!(latest.total_settled_volume_usd, 15_000_000);

    // Requesting more days than exist returns only what is stored.
    let history = client.get_stats_history(&30u32);
    assert_eq!(history.len(), 2);
}

#[test]
fn test_stats_history_rolling_window_caps_at_30_days() {
    let (env, client, admin) = setup();

    // Create 35 daily snapshots; only the most recent 30 should be retained.
    for day in 0..35u32 {
        env.ledger().set_sequence_number(day * DAY_LEDGERS);
        client.record_payment(&admin, &1_000_000i128, &true);
        client.snapshot_stats(&admin);
    }

    let history = client.get_stats_history(&30u32);
    assert_eq!(history.len(), 30);

    // The oldest retained snapshot should reflect day 5 (35 - 30).
    let oldest = history.get(0).unwrap();
    assert_eq!(oldest.total_payments, 6);
}

#[test]
fn test_prune_stats_history_removes_old_snapshots() {
    let (env, client, admin) = setup();

    for day in 0..10u32 {
        env.ledger().set_sequence_number(day * DAY_LEDGERS);
        client.record_payment(&admin, &1_000_000i128, &true);
        client.snapshot_stats(&admin);
    }
    assert_eq!(client.get_stats_history(&30u32).len(), 10);

    // Prune snapshots older than 5 days relative to the current ledger.
    env.ledger().set_sequence_number(9 * DAY_LEDGERS);
    client.prune_stats_history(&admin, &5u32);

    let history = client.get_stats_history(&30u32);
    assert_eq!(history.len(), 5);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_snapshot_stats_requires_admin() {
    let (env, client, _admin) = setup();
    let attacker = Address::generate(&env);
    client.snapshot_stats(&attacker);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_prune_stats_history_requires_admin() {
    let (env, client, _admin) = setup();
    let attacker = Address::generate(&env);
    client.prune_stats_history(&attacker, &5u32);
}
