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

/// A pool that no longer implements `AmmInterface` (e.g. after an upgrade):
/// it has no `get_reserves` entrypoint at all, so any cross-contract call to
/// it reverts. Used to verify graceful fallback instead of an uncontrolled panic.
#[contract]
pub struct BrokenAmm;

#[contractimpl]
impl BrokenAmm {
    pub fn something_else(_env: Env) -> u32 {
        0
    }
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

#[test]
fn test_initialize_sets_admin_and_requires_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let router_id = env.register_contract(None, LiquidityRouter);
    let router_client = LiquidityRouterClient::new(&env, &router_id);

    let admin = Address::generate(&env);
    router_client.initialize(&admin);

    // Admin is recorded and the allowlist starts empty.
    assert_eq!(router_client.get_admin(), admin);
    assert_eq!(router_client.get_approved_pools().len(), 0);
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_initialize_cannot_be_called_twice() {
    let env = Env::default();
    env.mock_all_auths();

    let router_id = env.register_contract(None, LiquidityRouter);
    let router_client = LiquidityRouterClient::new(&env, &router_id);

    let admin = Address::generate(&env);
    router_client.initialize(&admin);

    // A second call (e.g. by an attacker) must be rejected and must not
    // overwrite the admin or wipe the approved-pools allowlist.
    let attacker = Address::generate(&env);
    router_client.initialize(&attacker);
}

#[test]
fn test_admin_and_approved_pools_live_in_instance_storage() {
    let env = Env::default();
    env.mock_all_auths();

    let router_id = env.register_contract(None, LiquidityRouter);
    let router_client = LiquidityRouterClient::new(&env, &router_id);

    let admin = Address::generate(&env);
    router_client.initialize(&admin);

    // Admin and the approved-pools allowlist are config singletons and must be
    // kept in instance() storage so their TTL is bumped with every contract
    // call, rather than in persistent() storage where they could be archived.
    let instance = env.as_contract(&router_id, || env.storage().instance());
    assert!(instance.has(&DataKey::Admin));
    assert!(instance.has(&DataKey::ApprovedPools));

    let persistent = env.as_contract(&router_id, || env.storage().persistent());
    assert!(!persistent.has(&DataKey::Admin));
    assert!(!persistent.has(&DataKey::ApprovedPools));
}
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

#[test]
fn test_reverting_pool_falls_back_to_classic_dex() {
    let env = Env::default();
    env.mock_all_auths();

    // An approved pool that no longer implements `AmmInterface` (e.g. upgraded
    // to a contract without `get_reserves`). Calling `get_reserves` on it reverts.
    let pool_address = env.register_contract(None, BrokenAmm);
    let router_id = env.register_contract(None, LiquidityRouter);
    let router_client = LiquidityRouterClient::new(&env, &router_id);

    // A failed `get_reserves` call must be treated the same as a shallow pool:
    // fall back to `Route::StellarClassicDEX` instead of panicking the whole call.
    let route = router_client.check_and_route(&pool_address, &50i128);
    assert_eq!(route, Route::StellarClassicDEX);

    // The fallback path emits the same event as the depth-based fallback.
    let events = env.events().all();
    assert!(events.len() >= 1);

    let event = events.last().unwrap();
    assert_eq!(event.0, router_id);
    assert_eq!(event.1.len(), 1);
}
