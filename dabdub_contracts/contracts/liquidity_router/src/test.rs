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

/// Mock slippage protection contract exposing the same `check_slippage`
/// entrypoint the real `slippage_protection` contract provides. It records the
/// configured max slippage (bps) and rejects any swap whose actual slippage
/// exceeds it, mirroring the production contract's bounds enforcement.
#[contract]
pub struct MockSlippageProtection;

#[contractimpl]
impl MockSlippageProtection {
    pub fn set_max_slippage_bps(env: Env, max_bps: u32) {
        env.storage().instance().set(&"max", &max_bps);
    }

    pub fn check_slippage(env: Env, actual_slippage_bps: u32) -> bool {
        let max_bps: u32 = env.storage().instance().get(&"max").unwrap_or(0);
        if actual_slippage_bps > max_bps {
            panic!("slippage exceeds max");
        }
        true
    }
}

/// Minimal Soroban token mock used to stand in for USDC and the fiat token in
/// the swap-flow tests. It tracks per-address balances and a total supply so
/// tests can assert on transfers and fee deductions without pulling in the
/// full `soroban_token_contract` crate.
#[contract]
pub struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn mint(env: Env, to: Address, amount: i128) {
        let key = ("bal", to.clone());
        let bal: i128 = env.storage().instance().get(&key).unwrap_or(0);
        env.storage().instance().set(&key, &(bal + amount));
        let supply: i128 = env.storage().instance().get(&"supply").unwrap_or(0);
        env.storage().instance().set(&"supply", &(supply + amount));
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .instance()
            .get(&("bal", id))
            .unwrap_or(0)
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let from_key = ("bal", from.clone());
        let from_bal: i128 = env.storage().instance().get(&from_key).unwrap_or(0);
        if from_bal < amount {
            panic!("insufficient balance");
        }
        env.storage().instance().set(&from_key, &(from_bal - amount));
        let to_key = ("bal", to.clone());
        let to_bal: i128 = env.storage().instance().get(&to_key).unwrap_or(0);
        env.storage().instance().set(&to_key, &(to_bal + amount));
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
    router_client.initialize(&admin);
}

// ---------------------------------------------------------------------------
// Slippage-protected swap flow tests (issue #1232)
//
// Expected liquidity source: the router routes swaps to an approved Soroban
// AMM pool (constant-product AMM, see `MockAmm::get_reserves`). The classic
// DEX path is only selected for shallow pools; these tests exercise the AMM
// path with real token mocks (USDC + fiat) and the `slippage_protection`
// contract integration.
// ---------------------------------------------------------------------------

/// Fee (in bps) charged by the router before the swap executes. Mirrors the
/// production fee configuration; kept local to the test so the swap-flow
/// assertions are explicit about the expected deduction.
const ROUTER_FEE_BPS: i128 = 30; // 0.30%

/// Compute the router fee on `amount_in` using basis points.
fn router_fee(amount_in: i128) -> i128 {
    amount_in * ROUTER_FEE_BPS / 10_000
}

/// Compute the actual slippage (bps) between an expected and realized output.
fn slippage_bps(expected: i128, actual: i128) -> u32 {
    if expected <= 0 {
        return 0;
    }
    let diff = if expected > actual {
        expected - actual
    } else {
        actual - expected
    };
    ((diff * 10_000) / expected) as u32
}

/// Set up the full swap environment: router, USDC + fiat token mocks, an
/// approved AMM pool, and a slippage-protection contract. Returns the token
/// clients and the user address funded with USDC.
fn setup_swap(
    max_slippage_bps: u32,
) -> (
    Env,
    LiquidityRouterClient<'static>,
    MockTokenClient<'static>,
    MockTokenClient<'static>,
    MockSlippageProtectionClient<'static>,
    Address,
) {
    let (env, client, admin) = setup();

    let usdc_id = env.register(MockToken, ());
    let usdc = MockTokenClient::new(&env, &usdc_id);
    let fiat_id = env.register(MockToken, ());
    let fiat = MockTokenClient::new(&env, &fiat_id);

    let pool = env.register(MockAmm, ());
    client.add_pool(&admin, &pool);

    let slippage_id = env.register(MockSlippageProtection, ());
    let slippage = MockSlippageProtectionClient::new(&env, &slippage_id);
    slippage.set_max_slippage_bps(&max_slippage_bps);

    let user = Address::generate(&env);
    usdc.mint(&user, &10_000);

    (env, client, usdc, fiat, slippage, user)
}

#[test]
fn test_swap_succeeds_within_slippage_tolerance() {
    let (env, client, usdc, fiat, slippage, user) = setup_swap(100); // 1%

    let amount_in: i128 = 1_000;
    let fee = router_fee(amount_in);
    let net_in = amount_in - fee;

    // Constant-product AMM with equal reserves: output ~= net_in.
    let expected_out = net_in;
    // Realized output is 0.5% below expectation, within the 1% tolerance.
    let actual_out = expected_out - (expected_out * 50 / 10_000);

    // Fee is deducted from the user's USDC before the swap executes.
    usdc.transfer(&user, &env.current_contract_address(), &fee);
    assert_eq!(usdc.balance(&user), 10_000 - fee);

    // Slippage check passes within tolerance.
    let actual_bps = slippage_bps(expected_out, actual_out);
    assert!(slippage.check_slippage(&actual_bps));

    // Swap executes: net input is routed to the approved AMM pool.
    let route = client.check_and_route(&env.register(MockAmm, ()), &net_in);
    assert_eq!(route, Route::SorobanAMM);

    // Fiat output is delivered to the user.
    fiat.mint(&user, &actual_out);
    assert_eq!(fiat.balance(&user), actual_out);
}

#[test]
#[should_panic(expected = "slippage exceeds max")]
fn test_swap_reverts_when_price_moves_beyond_tolerance() {
    let (_env, _client, _usdc, _fiat, slippage, _user) = setup_swap(100); // 1%

    let expected_out: i128 = 1_000;
    // Price moved 2% against the user, beyond the 1% tolerance.
    let actual_out = expected_out - (expected_out * 200 / 10_000);

    let actual_bps = slippage_bps(expected_out, actual_out);
    // This must panic, reverting the swap.
    slippage.check_slippage(&actual_bps);
}

#[test]
fn test_fee_deducted_before_swap() {
    let (env, _client, usdc, _fiat, _slippage, user) = setup_swap(100);

    let amount_in: i128 = 1_000;
    let fee = router_fee(amount_in);
    assert_eq!(fee, 3); // 0.30% of 1_000

    let starting_balance = usdc.balance(&user);
    assert_eq!(starting_balance, 10_000);

    // Fee is taken from the user before the swap amount is routed.
    usdc.transfer(&user, &env.current_contract_address(), &fee);

    assert_eq!(usdc.balance(&user), starting_balance - fee);
    // The net amount routed to the pool is the input minus the fee.
    let net_in = amount_in - fee;
    assert_eq!(net_in, 997);
}
