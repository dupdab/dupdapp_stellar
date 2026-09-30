#![cfg(test)]

use super::*;
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, Env};

#[contract]
pub struct MockAmm;

#[contractimpl]
impl MockAmm {
    pub fn get_reserves(_env: Env) -> (i128, i128) {
        (100_000, 100_000)
    }
}

fn setup() -> (Env, LiquidityRouterClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(LiquidityRouter, ());
    let client = LiquidityRouterClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, client, admin)
}

#[test]
fn test_initialize_sets_admin_and_empty_allowlist() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(LiquidityRouter, ());
    let client = LiquidityRouterClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    // A pool that was never added must be rejected, proving the allowlist
    // was initialized to an empty set.
    let pool = env.register(MockAmm, ());
    let result = client.try_check_and_route(&pool, &1_000);
    assert!(result.is_err());
}

#[test]
fn test_add_pool_allows_routing() {
    let (env, client, admin) = setup();
    let pool = env.register(MockAmm, ());

    client.add_pool(&admin, &pool);

    let route = client.check_and_route(&pool, &1_000);
    assert_eq!(route, Route::SorobanAMM);
}

#[test]
fn test_add_pool_duplicate_is_noop() {
    let (env, client, admin) = setup();
    let pool = env.register(MockAmm, ());

    client.add_pool(&admin, &pool);
    // Adding the same pool again must be a no-op (no panic, still approved).
    client.add_pool(&admin, &pool);

    let route = client.check_and_route(&pool, &1_000);
    assert_eq!(route, Route::SorobanAMM);
}

#[test]
fn test_remove_pool_revokes_approval() {
    let (env, client, admin) = setup();
    let pool = env.register(MockAmm, ());

    client.add_pool(&admin, &pool);
    client.remove_pool(&admin, &pool);

    let result = client.try_check_and_route(&pool, &1_000);
    assert!(result.is_err());
}

#[test]
fn test_remove_pool_not_present_is_noop() {
    let (env, client, admin) = setup();
    let pool = env.register(MockAmm, ());

    // Removing a pool that was never added must not panic.
    client.remove_pool(&admin, &pool);

    let result = client.try_check_and_route(&pool, &1_000);
    assert!(result.is_err());
}

#[test]
#[should_panic(expected = "pool is not approved")]
fn test_check_and_route_panics_for_unapproved_pool() {
    let (env, client, _admin) = setup();
    let pool = env.register(MockAmm, ());

    // Pool was never added to the allowlist.
    client.check_and_route(&pool, &1_000);
}

#[test]
fn test_deep_pool_routing() {
    let (env, client, admin) = setup();
    let pool = env.register(MockAmm, ());
    client.add_pool(&admin, &pool);

    // amount_in (1_000) < reserve (100_000) / 10 => SorobanAMM
    let route = client.check_and_route(&pool, &1_000);
    assert_eq!(route, Route::SorobanAMM);
}

#[test]
fn test_shallow_pool_routing() {
    let (env, client, admin) = setup();
    let pool = env.register(MockAmm, ());
    client.add_pool(&admin, &pool);

    // amount_in (50_000) >= reserve (100_000) / 10 => StellarClassicDEX
    let route = client.check_and_route(&pool, &50_000);
    assert_eq!(route, Route::StellarClassicDEX);
}

#[test]
fn test_borderline_depth() {
    let (env, client, admin) = setup();
    let pool = env.register(MockAmm, ());
    client.add_pool(&admin, &pool);

    // amount_in (10_000) == reserve (100_000) / 10 => not strictly less => StellarClassicDEX
    let route = client.check_and_route(&pool, &10_000);
    assert_eq!(route, Route::StellarClassicDEX);
}

#[test]
fn test_default_depth_threshold_is_1000_bps() {
    let (env, client, admin) = setup();
    let pool = env.register(MockAmm, ());
    client.add_pool(&admin, &pool);

    // With no explicit configuration the threshold defaults to 1000 bps (10%),
    // preserving the previous hardcoded behavior.
    assert_eq!(client.get_depth_threshold_bps(), 1000);

    // amount_in (1_000) < reserve (100_000) * 1000 / 10_000 => SorobanAMM
    let route = client.check_and_route(&pool, &1_000);
    assert_eq!(route, Route::SorobanAMM);
}

#[test]
fn test_set_depth_threshold_bps_updates_routing() {
    let (env, client, admin) = setup();
    let pool = env.register(MockAmm, ());
    client.add_pool(&admin, &pool);

    // Raise the threshold to 5000 bps (50%).
    client.set_depth_threshold_bps(&admin, &5_000);
    assert_eq!(client.get_depth_threshold_bps(), 5_000);

    // amount_in (50_000) < reserve (100_000) * 5000 / 10_000 (50_000) is false,
    // so it still routes to the classic DEX at the boundary.
    let route = client.check_and_route(&pool, &50_000);
    assert_eq!(route, Route::StellarClassicDEX);

    // amount_in (40_000) < 50_000 => SorobanAMM under the raised threshold.
    let route = client.check_and_route(&pool, &40_000);
    assert_eq!(route, Route::SorobanAMM);
}

#[test]
fn test_set_depth_threshold_bps_lowers_threshold() {
    let (env, client, admin) = setup();
    let pool = env.register(MockAmm, ());
    client.add_pool(&admin, &pool);

    // Lower the threshold to 100 bps (1%).
    client.set_depth_threshold_bps(&admin, &100);

    // amount_in (1_000) < reserve (100_000) * 100 / 10_000 (1_000) is false,
    // so it now routes to the classic DEX.
    let route = client.check_and_route(&pool, &1_000);
    assert_eq!(route, Route::StellarClassicDEX);
}

#[test]
fn test_set_depth_threshold_bps_rejects_out_of_range() {
    let (_env, client, admin) = setup();

    // A threshold above 100% (10_000 bps) is invalid.
    let result = client.try_set_depth_threshold_bps(&admin, &10_001);
    assert!(result.is_err());
}
