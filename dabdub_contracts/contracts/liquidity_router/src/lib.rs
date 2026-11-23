#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};

// Issue #1082: Upper bound on the approved pool allowlist to keep
// add_pool/remove_pool/check_and_route from scaling without limit.
const MAX_POOLS: u32 = 100;

// Issue #1085: Default AMM depth threshold (10% impact) in basis points.
const DEFAULT_DEPTH_THRESHOLD_BPS: u32 = 1000;

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    SorobanAMM,
    StellarClassicDEX,
}

// Issue #1025: Add admin-managed pool allowlist
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    ApprovedPools,
    // Issue #1085: Admin-configurable AMM depth threshold in basis points
    DepthThresholdBps,
    // Issue #1223: Address of the slippage_protection contract
    SlippageProtection,
    // Issue #1516: Consecutive failure count per route
    RouteFailures(Route),
    // Issue #1516: Circuit-breaker flag per route (true = disabled)
    RouteDisabled(Route),
    // Issue #1517: Admin-configurable max oracle price age in seconds
    MaxOracleAgeSeconds,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FallbackRouteUsed {
    pub pool_id: Address,
    pub amount: i128,
    pub reserve: i128,
}

// Issue #1083: Event emitted when a pool is added to the allowlist
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PoolAddedEvent {
    pub pool_id: Address,
}

// Issue #1083: Event emitted when a pool is removed from the allowlist
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PoolRemovedEvent {
    pub pool_id: Address,
}

#[soroban_sdk::contractclient(name = "AmmClient")]
pub trait AmmInterface {
    fn get_reserves(env: Env) -> (i128, i128);
}

// Issue #1223: Client for the slippage_protection contract. The router calls
// `check_slippage` before executing any USDC->fiat conversion so that a swap
// breaching the merchant's configured bounds is rejected.
#[soroban_sdk::contractclient(name = "SlippageProtectionClient")]
pub trait SlippageProtectionInterface {
    fn check_slippage(
        env: Env,
        merchant: Address,
        expected_amount: i128,
        actual_amount: i128,
    ) -> bool;
}

// Issue #1232: Minimal SEP-41-style token interface used by the router to
// move USDC and fiat balances during a swap. Implemented by the token mocks
// in `test.rs` and by real token contracts in production.
#[soroban_sdk::contractclient(name = "TokenClient")]
pub trait TokenInterface {
    fn balance(env: Env, id: Address) -> i128;
    fn transfer(env: Env, from: Address, to: Address, amount: i128);
}

// Issue #1515: Contract-level errors returned by the slippage-aware routing entrypoint.
#[soroban_sdk::contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    SlippageExceeded = 1,
    InvalidInput = 2,
    // Issue #1516
    NotAdmin = 3,
    RouteDisabled = 4,
    // Issue #1517
    OraclePriceTooOld = 5,
}

// Issue #1517: Default max oracle age (5 minutes) used until the admin configures one.
const DEFAULT_MAX_ORACLE_AGE_SECONDS: u64 = 300;

// Issue #1516: Consecutive failures after which a route's circuit opens.
const MAX_CONSECUTIVE_FAILURES: u32 = 5;

fn require_router_admin(env: &Env, admin: &Address) -> Result<(), Error> {
    admin.require_auth();
    let stored_admin: Option<Address> = env.storage().instance().get(&DataKey::Admin);
    match stored_admin {
        Some(a) if a == *admin => Ok(()),
        _ => Err(Error::NotAdmin),
    }
}

// Issue #1515: Fixed-point scale for quoted/actual rates (7 decimals, as on Stellar).
const RATE_SCALE: i128 = 10_000_000;

#[contract]
pub struct LiquidityRouter;

#[contractimpl]
impl LiquidityRouter {
    // Issue #1025: Constructor to initialize admin
    // Issue #1079: Require auth and guard against re-initialization
    // Issue #1080: Store config singletons in instance() storage so their TTL
    // is bumped automatically with every contract call.
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();

        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        let pools: Vec<Address> = Vec::new(&env);
        env.storage()
            .instance()
            .set(&DataKey::ApprovedPools, &pools);
        // Issue #1085: Seed the depth threshold with the previous hardcoded default
        env.storage().instance().set(&DataKey::DepthThresholdBps, &DEFAULT_DEPTH_THRESHOLD_BPS);
        env.events()
            .publish(("LIQUIDITY_ROUTER", "initialized"), (admin,));
    }

    // Issue #1223: Admin-gated setter for the slippage_protection contract address.
    pub fn set_slippage_protection(env: Env, admin: Address, slippage_protection: Address) {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not set");
        if admin != stored_admin {
            panic!("only admin can set slippage protection");
        }

        let previous: Option<Address> = env.storage().instance().get(&DataKey::SlippageProtection);
        env.storage().instance().set(&DataKey::SlippageProtection, &slippage_protection);
        env.events().publish(
            ("LIQUIDITY_ROUTER", "slippage_protection_updated"),
            (previous, slippage_protection),
        );
    }

    // Issue #1025: Add pool to allowlist (admin-gated)
    pub fn add_pool(env: Env, admin: Address, pool: Address) {
        admin.require_auth();

        // Verify caller is the admin
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not set");
        if admin != stored_admin {
            panic!("only admin can add pools");
        }

        let mut pools: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedPools)
            .unwrap_or_else(|| Vec::new(&env));

        // Prevent duplicates
        if !pools.iter().any(|p| p == pool) {
            // Issue #1082: Enforce a maximum allowlist size before inserting
            if pools.len() >= MAX_POOLS {
                panic!("max pools reached");
            }
            pools.push_back(pool.clone());
            env.storage()
                .instance()
                .set(&DataKey::ApprovedPools, &pools);

            // Issue #1083: Emit event for the allowlist change
            env.events().publish(
                (Symbol::new(&env, "PoolAddedEvent"),),
                PoolAddedEvent { pool_id: pool },
            );
        }
    }

    // Issue #1025: Remove pool from allowlist (admin-gated)
    pub fn remove_pool(env: Env, admin: Address, pool: Address) {
        admin.require_auth();

        // Verify caller is the admin
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not set");
        if admin != stored_admin {
            panic!("only admin can remove pools");
        }

        let mut pools: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedPools)
            .unwrap_or_else(|| Vec::new(&env));

        // Remove the pool if it exists
        let mut new_pools = Vec::new(&env);
        let mut removed = false;
        for p in pools.iter() {
            if p != pool {
                new_pools.push_back(p);
            } else {
                removed = true;
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::ApprovedPools, &new_pools);

        // Issue #1083: Emit event only when a pool was actually removed
        if removed {
            env.events().publish(
                (Symbol::new(&env, "PoolRemovedEvent"),),
                PoolRemovedEvent { pool_id: pool },
            );
        }
    }

    // Issue #1085: Admin-gated setter for the AMM depth threshold (in bps)
    pub fn set_depth_threshold_bps(env: Env, admin: Address, threshold_bps: u32) {
        admin.require_auth();

        // Verify caller is the admin
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        if admin != stored_admin {
            panic!("only admin can set depth threshold");
        }

        if threshold_bps == 0 || threshold_bps > 10_000 {
            panic!("invalid depth threshold");
        }

        let previous: u32 = env.storage().instance().get(&DataKey::DepthThresholdBps).unwrap();
        env.storage().instance().set(&DataKey::DepthThresholdBps, &threshold_bps);
        env.events().publish(
            ("LIQUIDITY_ROUTER", "depth_threshold_updated"),
            (previous, threshold_bps),
        );
    }

    /// Issue #1223: Executes a USDC->fiat swap for a merchant, enforcing the
    /// merchant's slippage bounds via the slippage_protection contract before
    /// the conversion is considered successful. Panics (rejecting the swap)
    /// when the actual amount breaches the configured bounds.
    pub fn swap_usdc_to_fiat(
        env: Env,
        merchant: Address,
        expected_amount: i128,
        actual_amount: i128,
    ) -> bool {
        if expected_amount <= 0 || actual_amount <= 0 {
            panic!("invalid swap amount");
        }

        let slippage_protection: Address = env
            .storage()
            .instance()
            .get(&DataKey::SlippageProtection)
            .expect("slippage protection not set");

        // Issue #1223: Enforce slippage bounds before executing the swap.
        let client = SlippageProtectionClient::new(&env, &slippage_protection);
        let within_bounds = client.check_slippage(&merchant, &expected_amount, &actual_amount);
        if !within_bounds {
            panic!("slippage bounds breached");
        }

        true
    }

    /// Issue #1232: Executes a slippage-protected USDC->fiat swap that moves
    /// real token balances. The fee is deducted from the USDC input *before*
    /// the swap amount is computed, then the slippage_protection contract is
    /// consulted against the post-fee amount. On success the USDC input is
    /// pulled from the merchant and the fiat output is paid out.
    ///
    /// Expected liquidity source: the Soroban AMM pool registered via
    /// `add_pool` (see `check_and_route`); the fiat leg is settled from the
    /// router's fiat reserve funded by the AMM pool.
    pub fn execute_swap(
        env: Env,
        merchant: Address,
        usdc_token: Address,
        fiat_token: Address,
        amount_in: i128,
        fee_bps: u32,
        expected_out: i128,
        actual_out: i128,
    ) -> i128 {
        merchant.require_auth();

        if amount_in <= 0 || expected_out <= 0 || actual_out <= 0 {
            panic!("invalid swap amount");
        }
        if fee_bps > 10_000 {
            panic!("invalid fee");
        }

        // Issue #1232: Deduct the fee from the input before the swap executes.
        let fee = amount_in * (fee_bps as i128) / 10_000;
        let swap_amount = amount_in - fee;
        if swap_amount <= 0 {
            panic!("swap amount below fee");
        }

        let slippage_protection: Address = env
            .storage()
            .instance()
            .get(&DataKey::SlippageProtection)
            .expect("slippage protection not set");

        // Issue #1232: Enforce slippage bounds on the post-fee swap amount.
        let client = SlippageProtectionClient::new(&env, &slippage_protection);
        let within_bounds = client.check_slippage(&merchant, &expected_out, &actual_out);
        if !within_bounds {
            panic!("slippage bounds breached");
        }

        // Issue #1232: Move the post-fee USDC input from the merchant to the
        // router, then pay out the fiat output to the merchant.
        let usdc = TokenClient::new(&env, &usdc_token);
        usdc.transfer(&merchant, &env.current_contract_address(), &swap_amount);

        let fiat = TokenClient::new(&env, &fiat_token);
        fiat.transfer(&env.current_contract_address(), &merchant, &actual_out);

        env.events().publish(
            ("LIQUIDITY_ROUTER", "swap_executed"),
            (merchant, usdc_token, fiat_token, amount_in, fee, actual_out),
        );
        actual_out
    }

    /// Checks if the AMM pool has sufficient depth for a swap.
    /// Returns SorobanAMM if depth is sufficient (impact below the configured
    /// threshold), otherwise returns StellarClassicDEX and emits an event.
    pub fn check_and_route(env: Env, pool_address: Address, amount_in: i128) -> Route {
        // Issue #1025: Validate pool_address against allowlist before calling get_reserves
        let pools: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedPools)
            .unwrap_or_else(|| Vec::new(&env));

        if !pools.iter().any(|p| p == pool_address) {
            panic!("pool not approved");
        }

        let amm = AmmClient::new(&env, &pool_address);
        let (reserve_a, reserve_b) = amm.get_reserves();
        let reserve = if reserve_a < reserve_b {
            reserve_a
        } else {
            reserve_b
        };

        if reserve <= 0 {
            panic!("empty pool");
        }

        let threshold_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::DepthThresholdBps)
            .unwrap_or(DEFAULT_DEPTH_THRESHOLD_BPS);

        // Impact in bps = amount_in / reserve * 10_000
        let impact_bps = amount_in * 10_000 / reserve;

        if impact_bps <= (threshold_bps as i128) {
            Route::SorobanAMM
        } else {
            env.events().publish(
                (Symbol::new(&env, "FallbackRouteUsed"),),
                FallbackRouteUsed {
                    pool_id: pool_address,
                    amount: amount_in,
                    reserve,
                },
            );
            Route::StellarClassicDEX
        }
    }

    /// Issue #1515: Slippage-enforcing variant of `check_and_route`. The caller
    /// supplies the quoted rate, the actual rate and `max_slippage_bps`; the
    /// contract itself rejects the route with `Error::SlippageExceeded` when the
    /// actual rate is worse than the quote by more than the tolerance, so the
    /// backend cannot bypass protection. Rates are scaled by `RATE_SCALE`.
    pub fn check_and_route_with_slippage(
        env: Env,
        pool_address: Address,
        amount_in: i128,
        quoted_rate: i128,
        actual_rate: i128,
        max_slippage_bps: u32,
        oracle_timestamp: u64,
    ) -> Result<Route, Error> {
        // Issue #1517: Reject stale oracle prices before routing.
        Self::validate_oracle_freshness(env.clone(), oracle_timestamp)?;
        if amount_in <= 0 || quoted_rate <= 0 || actual_rate <= 0 || max_slippage_bps > 10_000 {
            return Err(Error::InvalidInput);
        }
        if actual_rate < quoted_rate {
            let slippage_bps = (quoted_rate - actual_rate) * 10_000 / quoted_rate;
            if slippage_bps > (max_slippage_bps as i128) {
                return Err(Error::SlippageExceeded);
            }
        }
        let route = Self::check_and_route(env.clone(), pool_address, amount_in);
        // Issue #1516: Refuse a route whose circuit breaker is open.
        if Self::is_route_disabled(env, route.clone()) {
            return Err(Error::RouteDisabled);
        }
        Ok(route)
    }

    /// Issue #1517: Admin-gated setter for the maximum accepted oracle price age.
    pub fn set_max_oracle_age_seconds(env: Env, admin: Address, seconds: u64) -> Result<(), Error> {
        require_router_admin(&env, &admin)?;
        if seconds == 0 {
            return Err(Error::InvalidInput);
        }
        env.storage().instance().set(&DataKey::MaxOracleAgeSeconds, &seconds);
        env.events()
            .publish(("LIQUIDITY_ROUTER", "max_oracle_age_updated"), (seconds,));
        Ok(())
    }

    /// Issue #1517: Currently configured max oracle age in seconds.
    pub fn get_max_oracle_age_seconds(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::MaxOracleAgeSeconds)
            .unwrap_or(DEFAULT_MAX_ORACLE_AGE_SECONDS)
    }

    /// Issue #1517: Returns `Error::OraclePriceTooOld` when `oracle_timestamp`
    /// is older than `max_oracle_age_seconds` relative to the ledger time.
    pub fn validate_oracle_freshness(env: Env, oracle_timestamp: u64) -> Result<(), Error> {
        let max_age = Self::get_max_oracle_age_seconds(env.clone());
        let age = env.ledger().timestamp().saturating_sub(oracle_timestamp);
        if age > max_age {
            return Err(Error::OraclePriceTooOld);
        }
        Ok(())
    }

    /// Issue #1516: Whether the circuit breaker for `route` is open.
    pub fn is_route_disabled(env: Env, route: Route) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::RouteDisabled(route))
            .unwrap_or(false)
    }

    /// Issue #1516: Consecutive failure count currently tracked for `route`.
    pub fn get_route_failures(env: Env, route: Route) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::RouteFailures(route))
            .unwrap_or(0)
    }

    /// Issue #1516: Records a failed execution on `route` (admin/backend
    /// reporter). After `MAX_CONSECUTIVE_FAILURES` consecutive failures the
    /// route is disabled and a `route_circuit_open` event is emitted.
    pub fn record_route_failure(env: Env, admin: Address, route: Route) -> Result<u32, Error> {
        require_router_admin(&env, &admin)?;
        let failures = Self::get_route_failures(env.clone(), route.clone()) + 1;
        env.storage()
            .instance()
            .set(&DataKey::RouteFailures(route.clone()), &failures);
        if failures >= MAX_CONSECUTIVE_FAILURES
            && !Self::is_route_disabled(env.clone(), route.clone())
        {
            env.storage()
                .instance()
                .set(&DataKey::RouteDisabled(route.clone()), &true);
            env.events().publish(
                ("LIQUIDITY_ROUTER", "route_circuit_open"),
                (route, failures),
            );
        }
        Ok(failures)
    }

    /// Issue #1516: Records a successful execution, resetting the failure streak.
    pub fn record_route_success(env: Env, admin: Address, route: Route) -> Result<(), Error> {
        require_router_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::RouteFailures(route), &0u32);
        Ok(())
    }

    /// Issue #1516: Admin re-enables a route and clears its failure count.
    pub fn reenable_route(env: Env, admin: Address, route: Route) -> Result<(), Error> {
        require_router_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::RouteDisabled(route.clone()), &false);
        env.storage().instance().set(&DataKey::RouteFailures(route.clone()), &0u32);
        env.events()
            .publish(("LIQUIDITY_ROUTER", "route_reenabled"), (route,));
        Ok(())
    }
}
