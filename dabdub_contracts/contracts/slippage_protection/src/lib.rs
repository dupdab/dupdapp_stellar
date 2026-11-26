#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec};

const DEFAULT_MAX_SLIPPAGE_BPS: u32 = 100; // 1%
const DEFAULT_TWAP_WINDOW: u32 = 10; // ledgers
const MAX_TWAP_WINDOW: u32 = 1_000; // ledgers

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    MaxSlippageBps,
    PairMaxSlippageBps(Address, Address),
    TwapWindow,
    PriceSample(Address, Address),
}

#[contracttype]
#[derive(Clone)]
pub struct PriceSample {
    pub price: i128,
    pub ledger: u32,
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

#[contracttype]
struct PairMaxSlippageUpdatedEvent {
    asset_a: Address,
    asset_b: Address,
    old_bps: u32,
    new_bps: u32,
}

#[contracttype]
struct TwapWindowUpdatedEvent {
    old_window: u32,
    new_window: u32,
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
        env.storage()
            .instance()
            .set(&DataKey::TwapWindow, &DEFAULT_TWAP_WINDOW);
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

    /// Records a price sample for (asset_a, asset_b) at the current ledger.
    /// Samples older than the TWAP window are pruned. Used to build the TWAP
    /// that slippage checks are validated against, so a single-transaction
    /// (flash-loan) spot manipulation cannot move the reference price.
    pub fn record_price(
        env: Env,
        asset_a: soroban_sdk::Address,
        asset_b: soroban_sdk::Address,
        price: i128,
    ) {
        if price <= 0 {
            panic!("price must be > 0");
        }
        let now = env.ledger().sequence();
        let window = Self::twap_window(&env);
        let key = DataKey::PriceSample(asset_a.clone(), asset_b.clone());
        let mut samples: Vec<PriceSample> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));

        // Prune samples that fall outside the TWAP window.
        let mut pruned: Vec<PriceSample> = Vec::new(&env);
        for s in samples.iter() {
            if now.saturating_sub(s.ledger) <= window {
                pruned.push_back(s);
            }
        }
        pruned.push_back(PriceSample { price, ledger: now });
        env.storage().persistent().set(&key, &pruned);
    }

    /// Returns the time-weighted average price for (asset_a, asset_b) over
    /// the configured TWAP window. Panics if no samples are available.
    pub fn get_twap(
        env: Env,
        asset_a: soroban_sdk::Address,
        asset_b: soroban_sdk::Address,
    ) -> i128 {
        Self::twap(&env, &asset_a, &asset_b)
    }

    /// Slippage check that validates `actual` against the TWAP reference
    /// price for the pair rather than the current spot price. This is the
    /// flash-loan-resistant variant: a spot price moved within a single
    /// transaction does not affect the TWAP reference.
    pub fn check_slippage_twap(
        env: Env,
        asset_a: soroban_sdk::Address,
        asset_b: soroban_sdk::Address,
        actual: i128,
    ) {
        let max_bps = Self::pair_max_slippage(&env, &asset_a, &asset_b);
        let twap = Self::twap(&env, &asset_a, &asset_b);
        Self::assert_within_tolerance(&env, twap, actual, max_bps);
    }

    /// Admin: update the maximum allowed slippage in basis points.
    pub fn set_max_slippage(env: Env, caller: soroban_sdk::Address, bps: u32) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        if bps > 10_000 {
            panic!("bps must be <= 10000");
        }

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
        if bps > 10_000 {
            panic!("bps must be <= 10000");
        }

        let old_bps = Self::pair_max_slippage(&env, &asset_a, &asset_b);
        env.storage().instance().set(
            &DataKey::PairMaxSlippageBps(asset_a.clone(), asset_b.clone()),
            &bps,
        );

        env.events().publish(
            ("SLIPPAGE_PROTECTION", "pair_max_slippage_set"),
            PairMaxSlippageUpdatedEvent {
                asset_a,
                asset_b,
                old_bps,
                new_bps: bps,
            },
        );
    }

    /// Admin: set the TWAP window (in ledgers) used for slippage reference
    /// prices. Must be between 1 and MAX_TWAP_WINDOW ledgers.
    pub fn set_twap_window(env: Env, caller: soroban_sdk::Address, window: u32) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        if window == 0 || window > MAX_TWAP_WINDOW {
            panic!("window out of bounds");
        }

        let old_window = Self::twap_window(&env);
        env.storage().instance().set(&DataKey::TwapWindow, &window);

        env.events().publish(
            ("SLIPPAGE_PROTECTION", "twap_window_set"),
            TwapWindowUpdatedEvent {
                old_window,
                new_window: window,
            },
        );
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

    pub fn get_twap_window(env: Env) -> u32 {
        Self::twap_window(&env)
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

    fn twap_window(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::TwapWindow)
            .unwrap_or(DEFAULT_TWAP_WINDOW)
    }

    /// Computes the time-weighted average price over the configured window.
    /// Each sample is weighted by the number of ledgers it was in effect
    /// (until the next sample, or until `now` for the latest sample).
    fn twap(env: &Env, asset_a: &Address, asset_b: &Address) -> i128 {
        let now = env.ledger().sequence();
        let window = Self::twap_window(env);
        let key = DataKey::PriceSample(asset_a.clone(), asset_b.clone());
        let samples: Vec<PriceSample> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env));

        let mut weighted_sum: i128 = 0;
        let mut total_weight: i128 = 0;
        let len = samples.len();
        for i in 0..len {
            let s = samples.get(i).unwrap();
            if now.saturating_sub(s.ledger) > window {
                continue;
            }
            let next_ledger = if i + 1 < len {
                samples.get(i + 1).unwrap().ledger
            } else {
                now
            };
            let weight = next_ledger.saturating_sub(s.ledger) as i128;
            if weight <= 0 {
                continue;
            }
            weighted_sum = weighted_sum
                .checked_add(s.price.checked_mul(weight).expect("overflow"))
                .expect("overflow");
            total_weight = total_weight.checked_add(weight).expect("overflow");
        }

        if total_weight == 0 {
            panic!("no price samples");
        }
        weighted_sum / total_weight
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
}
