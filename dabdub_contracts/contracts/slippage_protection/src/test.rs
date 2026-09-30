#![cfg(test)]

use crate::{SlippageProtectionContract, SlippageProtectionContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup_env() -> (Env, SlippageProtectionContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(SlippageProtectionContract, (&admin,));
    let client = SlippageProtectionContractClient::new(&env, &contract_id);

    (env, client, admin)
}

// ── constructor ──────────────────────────────────────────────────────────────

#[test]
fn test_constructor_defaults() {
    let (_env, client, admin) = setup_env();
    assert_eq!(client.get_max_slippage(), 100);
    assert_eq!(client.get_admin(), admin);
}

// ── check_slippage: happy paths ──────────────────────────────────────────────

#[test]
fn test_check_slippage_exact_boundary_passes() {
    // deviation == max_bps (100 bps = 1%) → should NOT revert
    let (_env, client, _admin) = setup_env();
    // actual is exactly 1% below expected: 10_000 → 9_900
    client.check_slippage(&10_000, &9_900);
}

#[test]
fn test_check_slippage_within_tolerance_passes() {
    let (_env, client, _admin) = setup_env();
    // 0.5% deviation — well within 1%
    client.check_slippage(&10_000, &9_950);
}

#[test]
fn test_check_slippage_zero_deviation_passes() {
    let (_env, client, _admin) = setup_env();
    client.check_slippage(&10_000, &10_000);
}

// ── check_slippage: revert paths ─────────────────────────────────────────────

#[test]
#[should_panic(expected = "SlippageExceeded")]
fn test_check_slippage_one_bps_over_reverts() {
    // deviation == 101 bps → must revert
    let (_env, client, _admin) = setup_env();
    // 101 bps below: 10_000 → 9_899
    client.check_slippage(&10_000, &9_899);
}

#[test]
#[should_panic(expected = "SlippageExceeded")]
fn test_check_slippage_large_deviation_reverts() {
    let (_env, client, _admin) = setup_env();
    client.check_slippage(&10_000, &5_000); // 50% deviation
}

#[test]
#[should_panic(expected = "expected must be > 0")]
fn test_check_slippage_zero_expected_reverts() {
    let (_env, client, _admin) = setup_env();
    client.check_slippage(&0, &100);
}

// ── set_max_slippage ─────────────────────────────────────────────────────────

#[test]
fn test_admin_can_tighten_slippage() {
    let (_env, client, admin) = setup_env();
    client.set_max_slippage(&admin, &50); // tighten to 0.5%
    assert_eq!(client.get_max_slippage(), 50);
}

#[test]
fn test_admin_can_relax_slippage() {
    let (_env, client, admin) = setup_env();
    client.set_max_slippage(&admin, &300); // relax to 3%
    assert_eq!(client.get_max_slippage(), 300);
}

#[test]
#[should_panic(expected = "bps must be <= 10000")]
fn test_admin_cannot_set_slippage_above_bps_denominator() {
    let (_env, client, admin) = setup_env();
    client.set_max_slippage(&admin, &10_001);
}

#[test]
fn test_tightened_slippage_rejects_previously_valid_swap() {
    let (_env, client, admin) = setup_env();
    // 0.8% deviation passes at default 1%
    client.check_slippage(&10_000, &9_920);
    // tighten to 0.5%
    client.set_max_slippage(&admin, &50);
    // now same deviation should revert — tested separately below
    assert_eq!(client.get_max_slippage(), 50);
}

#[test]
#[should_panic(expected = "SlippageExceeded")]
fn test_tightened_slippage_reverts_swap() {
    let (_env, client, admin) = setup_env();
    client.set_max_slippage(&admin, &50); // 0.5%
                                          // 0.8% deviation → should now revert
    client.check_slippage(&10_000, &9_920);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_non_admin_cannot_set_slippage() {
    let (env, client, _admin) = setup_env();
    let random = Address::generate(&env);
    client.set_max_slippage(&random, &200);
}
