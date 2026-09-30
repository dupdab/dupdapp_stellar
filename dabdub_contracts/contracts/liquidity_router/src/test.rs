#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Events, Env, Symbol};

#[contract]
pub struct MockAmm;

#[contractimpl]
impl MockAmm {
    pub fn get_reserves(env: Env) -> (i128, i128) {
        let res_a = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "res_a"))
            .unwrap_or(0);
        let res_b = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "res_b"))
            .unwrap_or(0);
        (res_a, res_b)
    }

    pub fn set_reserves(env: Env, res_a: i128, res_b: i128) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "res_a"), &res_a);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "res_b"), &res_b);
    }
}

#[test]
fn test_deep_pool_routing() {
    let env = Env::default();
    let pool_address = env.register_contract(None, MockAmm);
    let router_id = env.register_contract(None, LiquidityRouter);
    let router_client = LiquidityRouterClient::new(&env, &router_id);

    // Set high reserves: 1000
    let amm_client = MockAmmClient::new(&env, &pool_address);
    amm_client.set_reserves(&1000i128, &1000i128);

    // Swap 50 (5% of 1000) -> Should be deep enough (50 * 10 = 500 < 1000)
    let route = router_client.check_and_route(&pool_address, &50i128);
    assert_eq!(route, Route::SorobanAMM);

    // Check no fallback event emitted
    assert_eq!(env.events().all().len(), 0);
}

#[test]
fn test_shallow_pool_routing() {
    let env = Env::default();
    env.mock_all_auths();

    let pool_address = env.register_contract(None, MockAmm);
    let router_id = env.register_contract(None, LiquidityRouter);
    let router_client = LiquidityRouterClient::new(&env, &router_id);

    // Set low reserves: 100
    let amm_client = MockAmmClient::new(&env, &pool_address);
    amm_client.set_reserves(&100i128, &100i128);

    // Swap 20 (20% of 100) -> Should trigger fallback (20 * 10 = 200 >= 100)
    let route = router_client.check_and_route(&pool_address, &20i128);
    assert_eq!(route, Route::StellarClassicDEX);

    // Verify event
    let events = env.events().all();
    assert!(events.len() >= 1);

    let event = events.last().unwrap();
    assert_eq!(event.0, router_id);
    assert_eq!(event.1.len(), 1);
}

#[test]
fn test_borderline_depth() {
    let env = Env::default();
    let pool_address = env.register_contract(None, MockAmm);
    let router_id = env.register_contract(None, LiquidityRouter);
    let router_client = LiquidityRouterClient::new(&env, &router_id);

    // Set reserves: 100
    let amm_client = MockAmmClient::new(&env, &pool_address);
    amm_client.set_reserves(&100i128, &100i128);

    // Swap 10 (10% of 100) -> S * 10 < R ? 10 * 10 < 100 is False (100 < 100 is False)
    // So it should trigger fallback
    let route = router_client.check_and_route(&pool_address, &10i128);
    assert_eq!(route, Route::StellarClassicDEX);
}

#[test]
#[should_panic(expected = "invalid pool reserves")]
fn test_negative_reserves_rejected() {
    let env = Env::default();
    let pool_address = env.register_contract(None, MockAmm);
    let router_id = env.register_contract(None, LiquidityRouter);
    let router_client = LiquidityRouterClient::new(&env, &router_id);

    // A misbehaving pool returns a negative reserve
    let amm_client = MockAmmClient::new(&env, &pool_address);
    amm_client.set_reserves(&-100i128, &100i128);

    router_client.check_and_route(&pool_address, &50i128);
}

#[test]
#[should_panic(expected = "invalid pool reserves")]
fn test_zero_reserves_rejected() {
    let env = Env::default();
    let pool_address = env.register_contract(None, MockAmm);
    let router_id = env.register_contract(None, LiquidityRouter);
    let router_client = LiquidityRouterClient::new(&env, &router_id);

    // A pool with zero reserves is nonsensical and must be rejected
    let amm_client = MockAmmClient::new(&env, &pool_address);
    amm_client.set_reserves(&0i128, &100i128);

    router_client.check_and_route(&pool_address, &50i128);
}
