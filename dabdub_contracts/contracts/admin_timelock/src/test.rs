#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Ledger, Env, String};

fn setup() -> (Env, AdminTimelockContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_sequence_number(100);

    let admin = Address::generate(&env);
    let contract_id = env.register(AdminTimelockContract, (&admin,));
    let client = AdminTimelockContractClient::new(&env, &contract_id);

    (env, client, admin)
}

fn param(env: &Env) -> String {
    String::from_str(env, "fee_rate")
}

fn value(env: &Env) -> String {
    String::from_str(env, "200")
}

const VALID_DELAY: u32 = 17_280;

// ---------------------------------------------------------------------------
// schedule_change
// ---------------------------------------------------------------------------

#[test]
fn test_schedule_change_returns_id() {
    let (env, client, admin) = setup();
    let id = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);
    let change = client.get_change(&id);
    assert_eq!(change.status, ChangeStatus::Pending);
    assert_eq!(change.execute_after, 100 + VALID_DELAY);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_schedule_change_unauthorized() {
    let (env, client, _) = setup();
    let attacker = Address::generate(&env);
    client.schedule_change(&attacker, &param(&env), &value(&env), &VALID_DELAY);
}

// ---------------------------------------------------------------------------
// Issue #1050 — MIN_DELAY_LEDGERS floor is enforced
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "delay_ledgers must be >= 17280 (24 h)")]
fn test_schedule_change_below_min_delay_rejected() {
    let (env, client, admin) = setup();
    // 17_279 is one ledger below the floor — must be rejected
    client.schedule_change(&admin, &param(&env), &value(&env), &17_279);
}

#[test]
fn test_schedule_change_exact_min_delay_accepted() {
    let (env, client, admin) = setup();
    let id = client.schedule_change(&admin, &param(&env), &value(&env), &17_280);
    let change = client.get_change(&id);
    assert_eq!(change.status, ChangeStatus::Pending);
}

// ---------------------------------------------------------------------------
// apply_change — early execution rejected
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Delay period has not elapsed")]
fn test_apply_change_early_rejected() {
    let (env, client, admin) = setup();
    let id = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);

    // advance only half the delay — not enough
    env.ledger().set_sequence_number(100 + VALID_DELAY / 2);
    client.apply_change(&admin, &id);
}

// ---------------------------------------------------------------------------
// apply_change — late execution succeeds
// ---------------------------------------------------------------------------

#[test]
fn test_apply_change_after_delay_succeeds() {
    let (env, client, admin) = setup();
    let id = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);

    env.ledger().set_sequence_number(100 + VALID_DELAY);
    client.apply_change(&admin, &id);

    let change = client.get_change(&id);
    assert_eq!(change.status, ChangeStatus::Applied);
}

#[test]
#[should_panic(expected = "Change is not pending")]
fn test_apply_change_twice_panics() {
    let (env, client, admin) = setup();
    let id = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);

    env.ledger().set_sequence_number(100 + VALID_DELAY);
    client.apply_change(&admin, &id);
    client.apply_change(&admin, &id);
}

// ---------------------------------------------------------------------------
// cancel_change
// ---------------------------------------------------------------------------

#[test]
fn test_cancel_change_before_execution() {
    let (env, client, admin) = setup();
    let id = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);
    client.cancel_change(&admin, &id);

    let change = client.get_change(&id);
    assert_eq!(change.status, ChangeStatus::Cancelled);
}

#[test]
#[should_panic(expected = "Change is not pending")]
fn test_cancel_applied_change_panics() {
    let (env, client, admin) = setup();
    let id = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);

    env.ledger().set_sequence_number(100 + VALID_DELAY);
    client.apply_change(&admin, &id);
    client.cancel_change(&admin, &id);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_cancel_change_unauthorized() {
    let (env, client, admin) = setup();
    let id = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);
    let attacker = Address::generate(&env);
    client.cancel_change(&attacker, &id);
}

// ---------------------------------------------------------------------------
// multiple independent changes are tracked separately
// ---------------------------------------------------------------------------

#[test]
fn test_two_changes_have_distinct_ids() {
    let (env, client, admin) = setup();
    let id1 = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);
    let id2 = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);
    assert_ne!(id1, id2);
}

// ---------------------------------------------------------------------------
// Issue #1049 — admin rotation
// ---------------------------------------------------------------------------

#[test]
fn test_rotate_admin_transfers_control() {
    let (env, client, old_admin) = setup();
    let new_admin = Address::generate(&env);

    client.rotate_admin(&old_admin, &new_admin);

    assert_eq!(client.get_admin(), new_admin);

    // old admin can no longer schedule
    let result = std::panic::catch_unwind(|| {
        client.schedule_change(&old_admin, &param(&env), &value(&env), &VALID_DELAY);
    });
    // We cannot use catch_unwind in no_std; rely on the panic test below.
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_old_admin_cannot_act_after_rotation() {
    let (env, client, old_admin) = setup();
    let new_admin = Address::generate(&env);

    client.rotate_admin(&old_admin, &new_admin);
    client.schedule_change(&old_admin, &param(&env), &value(&env), &VALID_DELAY);
}

#[test]
fn test_new_admin_can_schedule_after_rotation() {
    let (env, client, old_admin) = setup();
    let new_admin = Address::generate(&env);

    client.rotate_admin(&old_admin, &new_admin);
    let id = client.schedule_change(&new_admin, &param(&env), &value(&env), &VALID_DELAY);
    let change = client.get_change(&id);
    assert_eq!(change.status, ChangeStatus::Pending);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_rotate_admin_unauthorized() {
    let (env, client, _) = setup();
    let attacker = Address::generate(&env);
    let new_admin = Address::generate(&env);
    client.rotate_admin(&attacker, &new_admin);
}

// ---------------------------------------------------------------------------
// Issue #1052 — list_pending
// ---------------------------------------------------------------------------

#[test]
fn test_list_pending_empty_initially() {
    let (_, client, _) = setup();
    assert_eq!(client.list_pending().len(), 0);
}

#[test]
fn test_list_pending_tracks_scheduled_changes() {
    let (env, client, admin) = setup();
    let id1 = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);
    let id2 = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);

    let pending = client.list_pending();
    assert_eq!(pending.len(), 2);
    assert!(pending.contains(&id1));
    assert!(pending.contains(&id2));
}

#[test]
fn test_list_pending_removes_applied_change() {
    let (env, client, admin) = setup();
    let id = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);

    env.ledger().set_sequence_number(100 + VALID_DELAY);
    client.apply_change(&admin, &id);

    assert_eq!(client.list_pending().len(), 0);
}

#[test]
fn test_list_pending_removes_cancelled_change() {
    let (env, client, admin) = setup();
    let id = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);
    client.cancel_change(&admin, &id);

    assert_eq!(client.list_pending().len(), 0);
}

// ---------------------------------------------------------------------------
// Issue #1051 — prune_change frees storage
// ---------------------------------------------------------------------------

#[test]
fn test_prune_applied_change_removes_from_storage() {
    let (env, client, admin) = setup();
    let id = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);

    env.ledger().set_sequence_number(100 + VALID_DELAY);
    client.apply_change(&admin, &id);
    client.prune_change(&admin, &id);

    // storage entry should be gone
    let result = std::panic::catch_unwind(|| client.get_change(&id));
    // rely on the panic test below for the no_std path
}

#[test]
#[should_panic(expected = "Change not found")]
fn test_get_change_after_prune_panics() {
    let (env, client, admin) = setup();
    let id = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);

    env.ledger().set_sequence_number(100 + VALID_DELAY);
    client.apply_change(&admin, &id);
    client.prune_change(&admin, &id);

    client.get_change(&id);
}

#[test]
fn test_prune_cancelled_change_removes_from_storage() {
    let (env, client, admin) = setup();
    let id = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);
    client.cancel_change(&admin, &id);
    client.prune_change(&admin, &id);
    // implicit: no panic — prune accepted
}

#[test]
#[should_panic(expected = "Cannot prune a pending change")]
fn test_prune_pending_change_panics() {
    let (env, client, admin) = setup();
    let id = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);
    client.prune_change(&admin, &id);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_prune_change_unauthorized() {
    let (env, client, admin) = setup();
    let id = client.schedule_change(&admin, &param(&env), &value(&env), &VALID_DELAY);

    env.ledger().set_sequence_number(100 + VALID_DELAY);
    client.apply_change(&admin, &id);

    let attacker = Address::generate(&env);
    client.prune_change(&attacker, &id);
}
