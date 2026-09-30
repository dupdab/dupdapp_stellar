#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

const DEFAULT_MAX_SLIPPAGE_BPS: u32 = 100; // 1%

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    MaxSlippageBps,
    PairMaxSlippageBps(Address, Address),
}

#[contracttype]
struct SlippageExceededEvent {
    expected: i128,
    actual: i128,
    max_bps: u32,
}

#[contracttype]
struct MaxSlippageUpdatedEvent {
    old_bps: u32,
    new_bps: u32,
}

#[contract]
pub struct SlippageProtectionContract;

#[contractimpl]
impl SlippageProtectionContract {
    pub fn __constructor(env: Env, admin: soroban_sdk::Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::MaxSlippageBps, &DEFAULT_MAX_SLIPPAGE_BPS);
    }

    /// Reverts if the actual rate deviates from expected by more than max_slippage_bps.
    /// expected and actual are prices in the same unit (e.g. stroops per USDC).
    pub fn check_slippage(env: Env, expected: i128, actual: i128) {
        let max_bps = Self::global_max_slippage(&env);
        Self::assert_within_tolerance(&env, expected, actual, max_bps);
    }

    /// Same as `check_slippage`, but uses the per-pair override for
    /// (asset_a, asset_b) when one has been set, falling back to the
    /// global `MaxSlippageBps` otherwise.
    pub fn check_slippage_for_pair(
        env: Env,
        asset_a: soroban_sdk::Address,
        asset_b: soroban_sdk::Address,
        expected: i128,
        actual: i128,
    ) {
        let max_bps = Self::pair_max_slippage(&env, &asset_a, &asset_b);
        Self::assert_within_tolerance(&env, expected, actual, max_bps);
    }

    /// Admin: update the maximum allowed slippage in basis points.
    pub fn set_max_slippage(env: Env, caller: soroban_sdk::Address, bps: u32) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let old_bps = Self::global_max_slippage(&env);
        env.storage().instance().set(&DataKey::MaxSlippageBps, &bps);

        env.events().publish(
            ("SLIPPAGE_PROTECTION", "max_slippage_set"),
            MaxSlippageUpdatedEvent {
                old_bps,
                new_bps: bps,
            },
        );
    }

    /// Admin: set a per-pair override for the maximum allowed slippage in
    /// basis points. The pair is unordered from the caller's perspective;
    /// lookups are keyed on the exact (asset_a, asset_b) order passed in.
    pub fn set_pair_max_slippage(
        env: Env,
        caller: soroban_sdk::Address,
        asset_a: soroban_sdk::Address,
        asset_b: soroban_sdk::Address,
        bps: u32,
    ) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        env.storage()
            .instance()
            .set(&DataKey::PairMaxSlippageBps(asset_a, asset_b), &bps);
    }

    pub fn get_max_slippage(env: Env) -> u32 {
        Self::global_max_slippage(&env)
    }

    /// Returns the effective max slippage (bps) for the given pair: the
    /// per-pair override if one exists, otherwise the global default.
    pub fn get_pair_max_slippage(
        env: Env,
        asset_a: soroban_sdk::Address,
        asset_b: soroban_sdk::Address,
    ) -> u32 {
        Self::pair_max_slippage(&env, &asset_a, &asset_b)
    }

    pub fn get_admin(env: Env) -> soroban_sdk::Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    fn require_admin(env: &Env, caller: &soroban_sdk::Address) {
        let admin: soroban_sdk::Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if caller != &admin {
            panic!("Not admin");
        }
    }

    fn global_max_slippage(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MaxSlippageBps)
            .unwrap_or(DEFAULT_MAX_SLIPPAGE_BPS)
    }

    /// Resolves the effective max slippage for a pair: the per-pair
    /// override when present, otherwise the global default.
    fn pair_max_slippage(env: &Env, asset_a: &Address, asset_b: &Address) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::PairMaxSlippageBps(
                asset_a.clone(),
                asset_b.clone(),
            ))
            .unwrap_or_else(|| Self::global_max_slippage(env))
    }

    fn assert_within_tolerance(env: &Env, expected: i128, actual: i128, max_bps: u32) {
        if expected <= 0 {
            panic!("expected must be > 0");
        }

        // deviation_bps = abs(expected - actual) * 10_000 / expected
        let diff = if actual >= expected {
            actual - expected
        } else {
            expected - actual
        };

        let deviation_bps_i128 = diff
            .checked_mul(10_000)
            .expect("overflow")
            .checked_div(expected)
            .expect("div by zero");

        // Saturate instead of truncating-casting: an i128 deviation that
        // overflows u32 is always far beyond any sane max_bps, so clamping
        // to u32::MAX preserves the "reject extreme deviation" behavior
        // instead of silently wrapping into a small, passable value.
        let deviation_bps: u32 = deviation_bps_i128.try_into().unwrap_or(u32::MAX);

        if deviation_bps > max_bps {
            env.events().publish(
                ("SLIPPAGE", "exceeded"),
                SlippageExceededEvent {
                    expected,
                    actual,
                    max_bps,
                },
            );
            panic!("SlippageExceeded");
        }
    }
    /// Admin: update the maximum allowed slippage in basis points.
    pub fn set_max_slippage(env: Env, caller: soroban_sdk::Address, bps: u32) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        if bps > 10_000 {
            panic!("bps must be <= 10000");
        }
        env.storage().instance().set(&DataKey::MaxSlippageBps, &bps);
    }

    pub fn get_max_slippage(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MaxSlippageBps)
            .unwrap_or(DEFAULT_MAX_SLIPPAGE_BPS)
    }

    pub fn get_admin(env: Env) -> soroban_sdk::Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    fn require_admin(env: &Env, caller: &soroban_sdk::Address) {
        let admin: soroban_sdk::Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if caller != &admin {
            panic!("Not admin");
        }
    }

}
