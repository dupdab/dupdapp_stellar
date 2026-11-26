#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup(env: &Env) -> (SlippageProtectionContractClient<'static>, Address) {
    let contract_id = env.register_contract(None, SlippageProtectionContract);
    let client = SlippageProtectionContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

#[test]
fn test_initialize_sets_default_twap_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    assert_eq!(client.get_twap_window(), DEFAULT_TWAP_WINDOW);
}

#[test]
fn test_admin_can_update_twap_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    client.set_twap_window(&5u32);
    assert_eq!(client.get_twap_window(), 5u32);
}

#[test]
#[should_panic]
fn test_twap_window_below_minimum_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    client.set_twap_window(&0u32);
}

#[test]
#[should_panic]
fn test_twap_window_above_maximum_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    client.set_twap_window(&(MAX_TWAP_WINDOW + 1));
}

#[test]
fn test_spot_price_within_twap_passes() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    // Record a stable price history around 100.
    for _ in 0..DEFAULT_TWAP_WINDOW {
        client.record_price(&100i128);
    }

    // Spot price is close to TWAP; a 1% slippage tolerance should pass.
    assert!(client.check_slippage(&101i128, &100i128));
}

#[test]
fn test_flash_loan_spot_manipulation_rejected_by_twap() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    // Establish a stable TWAP of ~100 over the window.
    for _ in 0..DEFAULT_TWAP_WINDOW {
        client.record_price(&100i128);
    }

    // Flash-loan style manipulation: spot price is pumped to 200 within a
    // single transaction. Under the old spot-based check this would be
    // compared directly against the expected price and could bypass
    // slippage protection. Under TWAP the manipulated spot is compared
    // against the time-weighted average (~100), so it is rejected.
    let manipulated_spot = 200i128;
    let expected_price = 100i128;

    assert!(!client.check_slippage(&manipulated_spot, &expected_price));
}

#[test]
fn test_twap_ignores_single_manipulated_ledger() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    // Stable history of 100.
    for _ in 0..DEFAULT_TWAP_WINDOW {
        client.record_price(&100i128);
    }

    // A single manipulated ledger at 1000 should not move the TWAP enough
    // to allow a manipulated spot price through.
    client.record_price(&1000i128);

    let twap = client.get_twap();
    // TWAP stays well below the manipulated spot.
    assert!(twap < 200i128);
    assert!(!client.check_slippage(&1000i128, &100i128));
}
