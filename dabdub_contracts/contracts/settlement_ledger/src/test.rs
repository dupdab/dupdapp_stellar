#![cfg(test)]

use fee_calculator::{FeeCalculatorContract, FeeCalculatorContractClient, FeeTier};
use soroban_sdk::{testutils::{Address as _, Ledger as _}, vec, Address, BytesN, Env, String};

use crate::{Error, SettlementLedgerContract, SettlementLedgerContractClient};

fn pid(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn fiat_ref(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

fn currency(env: &Env, code: &str) -> String {
    String::from_str(env, code)
}

fn record_with_currency(
    client: &SettlementLedgerContractClient,
    admin: &Address,
    env: &Env,
    seed: u8,
    merchant: &Address,
    amount: i128,
    fee: i128,
    cur: &str,
) {
    client.record_settlement(
        admin,
        &pid(env, seed),
        merchant,
        &amount,
        &fee,
        &(amount - fee),
        &1_700_000_000u64,
        &fiat_ref(env, "REF-001"),
        &currency(env, cur),
    );
}

fn setup() -> (Env, Address, SettlementLedgerContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let tiers = vec![&env, FeeTier { threshold_usdc: 0, fee_bps: 200 }];
    let fee_id = env.register(FeeCalculatorContract, (&admin, tiers, &10u32));
    let id = env.register(SettlementLedgerContract, (&admin, &fee_id));
    let fee_client = FeeCalculatorContractClient::new(&env, &fee_id);
    fee_client.set_settlement_caller(&admin, &id);
    let client = SettlementLedgerContractClient::new(&env, &id);
    (env, admin, client)
}

fn record(
    client: &SettlementLedgerContractClient,
    admin: &Address,
    env: &Env,
    seed: u8,
    merchant: &Address,
    amount: i128,
    fee: i128,
) {
    client.record_settlement(
        admin,
        &pid(env, seed),
        merchant,
        &amount,
        &fee,
        &(amount - fee),
        &1_700_000_000u64,
        &fiat_ref(env, "REF-001"),
        &currency(env, "NGN"),
    );
}

// ── basic record + get ────────────────────────────────────────────────────────

#[test]
fn test_record_and_get() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);

    client.record_settlement(
        &admin,
        &pid(&env, 1),
        &merchant,
        &10_000,
        &200,
        &9_800,
        &1_700_000_000u64,
        &fiat_ref(&env, "TXN-ABC"),
        &currency(&env, "NGN"),
    );

    let r = client.get_settlement(&pid(&env, 1));
    assert_eq!(r.amount, 10_000);
    assert_eq!(r.fee, 200);
    assert_eq!(r.net, 9_800);
    assert_eq!(r.timestamp, 1_700_000_000u64);
    assert_eq!(r.fiat_currency, currency(&env, "NGN"));
}

// ── administration ──────────────────────────────────────────────────────────

#[test]
fn test_admin_rotation_allows_new_admin_to_record() {
    let (env, admin, client) = setup();
    let new_admin = Address::generate(&env);
    let merchant = Address::generate(&env);

    client.transfer_admin(&admin, &new_admin);
    record(&client, &new_admin, &env, 5, &merchant, 10_000, 200);

    assert_eq!(client.settlement_count(&merchant), 1);
}

// ── idempotency: replaying a payment_id returns the existing entry ───────────

#[test]
fn test_duplicate_record_is_idempotent() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);
    record(&client, &admin, &env, 2, &merchant, 5_000, 100);

    // Replay with different non-key fields: the original entry is returned.
    let replayed = client.record_settlement(
        &admin,
        &pid(&env, 2),
        &merchant,
        &5_000,
        &100,
        &4_900,
        &1_700_000_001u64,
        &fiat_ref(&env, "DUP"),
        &currency(&env, "NGN"),
    );
    assert_eq!(replayed.timestamp, 1_700_000_000u64);
    assert_eq!(replayed.fiat_ref, fiat_ref(&env, "REF-001"));
    assert_eq!(client.settlement_count(&merchant), 1);
    assert_eq!(client.total_settlement_count(), 1);
}

// ── archival of old settlements ─────────────────────────────────────────────

#[test]
fn test_archive_settlements_moves_old_records() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);
    record(&client, &admin, &env, 80, &merchant, 5_000, 100);
    env.ledger().with_mut(|l| l.sequence_number += 100);
    record(&client, &admin, &env, 81, &merchant, 5_000, 100);

    // Only the first settlement is at least 50 ledgers old.
    assert_eq!(client.archive_settlements(&admin, &50, &10), 1);
    assert!(client.is_archived(&pid(&env, 80)));
    assert!(!client.is_archived(&pid(&env, 81)));

    // Still readable and still listed after archival.
    assert_eq!(client.get_settlement(&pid(&env, 80)).amount, 5_000);
    assert_eq!(client.list_settlements(&merchant, &0).len(), 2);

    // Nothing left to archive on a second pass.
    assert_eq!(client.archive_settlements(&admin, &50, &10), 0);
}

#[test]
fn test_archive_settlements_respects_limit() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);
    record(&client, &admin, &env, 82, &merchant, 5_000, 100);
    record(&client, &admin, &env, 83, &merchant, 5_000, 100);
    env.ledger().with_mut(|l| l.sequence_number += 100);

    assert_eq!(client.archive_settlements(&admin, &50, &1), 1);
    assert_eq!(client.archive_settlements(&admin, &50, &1), 1);
    assert_eq!(client.archive_settlements(&admin, &50, &1), 0);
}

#[test]
fn test_archive_settlements_unauthorized_caller_fails() {
    let (env, _admin, client) = setup();
    let caller = Address::generate(&env);
    assert!(client.try_archive_settlements(&caller, &0, &10).is_err());
}

// ── merchant settlement history (cursor pagination) ─────────────────────────

#[test]
fn test_get_merchant_settlements_paginates_with_cursor() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);
    let other = Address::generate(&env);
    for seed in 100u8..105 {
        record(&client, &admin, &env, seed, &merchant, 5_000, 100);
    }
    record(&client, &admin, &env, 110, &other, 5_000, 100);

    let first = client.get_merchant_settlements(&merchant, &2, &0);
    assert_eq!(first.settlement_ids.len(), 2);
    assert_eq!(first.settlement_ids.get(0).unwrap(), pid(&env, 100));
    assert_eq!(first.settlement_ids.get(1).unwrap(), pid(&env, 101));
    assert_eq!(first.next_cursor, Some(2));

    let second = client.get_merchant_settlements(&merchant, &2, &first.next_cursor.unwrap());
    assert_eq!(second.settlement_ids.get(0).unwrap(), pid(&env, 102));
    assert_eq!(second.next_cursor, Some(4));

    let last = client.get_merchant_settlements(&merchant, &2, &4);
    assert_eq!(last.settlement_ids.len(), 1);
    assert_eq!(last.settlement_ids.get(0).unwrap(), pid(&env, 104));
    assert_eq!(last.next_cursor, None);
}

#[test]
fn test_get_merchant_settlements_empty_and_past_end() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);
    let empty = client.get_merchant_settlements(&merchant, &10, &0);
    assert_eq!(empty.settlement_ids.len(), 0);
    assert_eq!(empty.next_cursor, None);

    record(&client, &admin, &env, 120, &merchant, 5_000, 100);
    let past = client.get_merchant_settlements(&merchant, &10, &5);
    assert_eq!(past.settlement_ids.len(), 0);
    assert_eq!(past.next_cursor, None);
}

// ── escrow amount validation ────────────────────────────────────────────────

fn record_checked(
    client: &SettlementLedgerContractClient,
    admin: &Address,
    env: &Env,
    seed: u8,
    merchant: &Address,
    escrow_release_amount: i128,
) -> Result<(), Error> {
    let result = client.try_record_settlement_checked(
        admin,
        &pid(env, seed),
        merchant,
        &5_000,
        &100,
        &4_900,
        &1_700_000_000u64,
        &fiat_ref(env, "REF-ESC"),
        &currency(env, "NGN"),
        &escrow_release_amount,
    );
    match result {
        Ok(_) => Ok(()),
        Err(Ok(err)) => Err(err),
        Err(Err(_)) => panic!("unexpected host error"),
    }
}

#[test]
fn test_escrow_check_allows_amount_up_to_release_amount() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);

    // Boundary: amount == release amount, and amount < release amount.
    assert!(record_checked(&client, &admin, &env, 130, &merchant, 5_000).is_ok());
    assert!(record_checked(&client, &admin, &env, 131, &merchant, 6_000).is_ok());
    assert_eq!(client.get_settlement(&pid(&env, 130)).amount, 5_000);
}

#[test]
fn test_escrow_check_rejects_amount_above_release_amount() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);

    let result = record_checked(&client, &admin, &env, 132, &merchant, 4_999);
    assert_eq!(result, Err(Error::AmountMismatch));
    assert_eq!(client.settlement_count(&merchant), 0);

    let zero = record_checked(&client, &admin, &env, 133, &merchant, 0);
    assert_eq!(zero, Err(Error::AmountMismatch));
}

#[test]
#[should_panic(expected = "not admin")]
fn test_record_settlement_unauthorized_caller_panics() {
    let (env, _admin, client) = setup();
    let caller = Address::generate(&env);
    let merchant = Address::generate(&env);
    record(&client, &caller, &env, 8, &merchant, 1_000, 20);
}

#[test]
fn test_void_preserves_original_settlement() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);
    record(&client, &admin, &env, 7, &merchant, 1_000, 20);
    client.void_settlement(&admin, &pid(&env, 7), &fiat_ref(&env, "operator correction"));
    assert_eq!(client.get_settlement(&pid(&env, 7)).amount, 1_000);
    assert_eq!(client.get_settlement_void(&pid(&env, 7)).unwrap().reason, fiat_ref(&env, "operator correction"));
}

// ── fee + net must equal amount ───────────────────────────────────────────────

#[test]
fn test_invariant_fee_plus_net_equals_amount() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);

    let result = client.try_record_settlement(
        &admin,
        &pid(&env, 3),
        &merchant,
        &10_000,
        &300,
        &9_500, // wrong: 300 + 9500 != 10000
        &1_700_000_000u64,
        &fiat_ref(&env, "BAD"),
        &currency(&env, "NGN"),
    );
    assert!(result.is_err());
}

// ── zero fee (fee-free merchant) ──────────────────────────────────────────────

#[test]
fn test_configured_fee() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);

    client.record_settlement(
        &admin,
        &pid(&env, 4),
        &merchant,
        &8_000,
        &160,
        &7_840,
        &1_700_000_000u64,
        &fiat_ref(&env, "FREE"),
        &currency(&env, "NGN"),
    );

    let r = client.get_settlement(&pid(&env, 4));
    assert_eq!(r.fee, 160);
    assert_eq!(r.net, 7_840);
}

// ── pagination ────────────────────────────────────────────────────────────────

#[test]
fn test_list_settlements_pagination() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);

    // Record 25 settlements for the same merchant (seeds 10..34)
    for seed in 10u8..35 {
        record(&client, &admin, &env, seed, &merchant, 1_000, 20);
    }

    assert_eq!(client.settlement_count(&merchant), 25);

    let page0 = client.list_settlements(&merchant, &0);
    assert_eq!(page0.len(), 20); // full page

    let page1 = client.list_settlements(&merchant, &1);
    assert_eq!(page1.len(), 5); // remainder

    let page2 = client.list_settlements(&merchant, &2);
    assert_eq!(page2.len(), 0); // beyond end
}

// ── different merchants are isolated ─────────────────────────────────────────

#[test]
fn test_merchant_isolation() {
    let (env, admin, client) = setup();
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);

    record(&client, &admin, &env, 50, &m1, 2_000, 40);
    record(&client, &admin, &env, 51, &m1, 2_000, 40);
    record(&client, &admin, &env, 52, &m2, 3_000, 60);

    assert_eq!(client.settlement_count(&m1), 2);
    assert_eq!(client.settlement_count(&m2), 1);
}

#[test]
fn test_platform_wide_index_lists_every_merchant() {
    let (env, admin, client) = setup();
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    record(&client, &admin, &env, 60, &first, 1_000, 20);
    record(&client, &admin, &env, 61, &second, 2_000, 40);
    assert_eq!(client.total_settlement_count(), 2);
    assert_eq!(client.list_all_settlements(&0).len(), 2);
}

// ── get on unknown payment_id panics ─────────────────────────────────────────

#[test]
fn test_get_unknown_panics() {
    let (env, _admin, client) = setup();
    let result = client.try_get_settlement(&pid(&env, 99));
    assert!(result.is_err());
}

// ── settlement reversal ───────────────────────────────────────────────────────

#[test]
fn test_record_reversal_references_original() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);
    record(&client, &admin, &env, 70, &merchant, 10_000, 200);

    client.record_reversal(&admin, &pid(&env, 70), &fiat_ref(&env, "chargeback"));

    let rev = client.get_reversal(&pid(&env, 70)).unwrap();
    assert_eq!(rev.original_payment_id, pid(&env, 70));
    assert_eq!(rev.reason, fiat_ref(&env, "chargeback"));
    assert_eq!(rev.merchant, merchant);
}

// ── maximum entries per merchant ──────────────────────────────────────────────

/// Number of settlements recorded per merchant in the max-count tests.
/// Kept modest so the suite stays fast while still exercising the
/// pagination boundary (multiple full pages plus a remainder).
const MAX_ENTRIES: u8 = 45;

fn fill_to_max(
    client: &SettlementLedgerContractClient,
    admin: &Address,
    env: &Env,
    merchant: &Address,
) {
    for seed in 0u8..MAX_ENTRIES {
        record(client, admin, env, seed, merchant, 1_000, 20);
    }
}

#[test]
fn test_settlement_at_max_count_returns_correct_response() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);

    fill_to_max(&client, &admin, &env, &merchant);
    assert_eq!(client.settlement_count(&merchant), MAX_ENTRIES as u32);

    // The most recently recorded settlement must still be retrievable and
    // return the exact values it was recorded with.
    let last_seed = MAX_ENTRIES - 1;
    let r = client.get_settlement(&pid(&env, last_seed));
    assert_eq!(r.amount, 1_000);
    assert_eq!(r.fee, 20);
    assert_eq!(r.net, 980);
    assert_eq!(r.merchant, merchant);

    // Every recorded settlement remains individually addressable at max count.
    for seed in 0u8..MAX_ENTRIES {
        let entry = client.get_settlement(&pid(&env, seed));
        assert_eq!(entry.amount, 1_000);
        assert_eq!(entry.merchant, merchant);
    }
}

#[test]
fn test_pagination_at_max_count() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);

    fill_to_max(&client, &admin, &env, &merchant);
    assert_eq!(client.settlement_count(&merchant), MAX_ENTRIES as u32);

    // Page size is 20: 45 entries => 20 + 20 + 5.
    let page0 = client.list_settlements(&merchant, &0);
    assert_eq!(page0.len(), 20);

    let page1 = client.list_settlements(&merchant, &1);
    assert_eq!(page1.len(), 20);

    let page2 = client.list_settlements(&merchant, &2);
    assert_eq!(page2.len(), 5);

    let page3 = client.list_settlements(&merchant, &3);
    assert_eq!(page3.len(), 0);

    // Pages must not overlap: every payment id appears exactly once.
    let mut seen = 0u32;
    for page in [page0, page1, page2] {
        seen += page.len();
    }
    assert_eq!(seen, MAX_ENTRIES as u32);
}

#[test]
fn test_archival_frees_space_for_new_settlements() {
    let (env, admin, client) = setup();
    let merchant = Address::generate(&env);

    fill_to_max(&client, &admin, &env, &merchant);
    assert_eq!(client.settlement_count(&merchant), MAX_ENTRIES as u32);

    // Archive an existing settlement; the merchant's active count drops by one.
    client.archive_settlement(&admin, &pid(&env, 0));
    assert_eq!(client.settlement_count(&merchant), (MAX_ENTRIES - 1) as u32);

    // The freed slot can be reused by a brand-new settlement.
    let new_seed = MAX_ENTRIES;
    record(&client, &admin, &env, new_seed, &merchant, 2_000, 40);
    assert_eq!(client.settlement_count(&merchant), MAX_ENTRIES as u32);

    let r = client.get_settlement(&pid(&env, new_seed));
    assert_eq!(r.amount, 2_000);
    assert_eq!(r.fee, 40);
    assert_eq!(r.net, 1_960);
    assert_eq!(r.merchant, merchant);
}
