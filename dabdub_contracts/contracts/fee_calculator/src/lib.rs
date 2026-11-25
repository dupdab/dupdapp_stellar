#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, vec, Address, Env, Vec};

/// Hard ceiling for a single-transaction fee amount in stroops.
/// Prevents an integer overflow in `amount * fee_bps` when both are large.
/// i128::MAX / BPS_DENOMINATOR(10_000) ≈ 1.7 × 10^34, well above any real
/// Stellar balance, but we cap earlier to keep math well within safe range.
const MAX_AMOUNT_FOR_FEE: i128 = i128::MAX / BPS_DENOMINATOR;

const LEDGERS_PER_30_DAYS: u32 = 172_800;
const BPS_DENOMINATOR: i128 = 10_000;

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FeeTier {
    pub threshold_usdc: i128,
    pub fee_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct MerchantVolume {
    pub window_start_ledger: u32,
    pub volume_usdc: i128,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    SettlementCaller,
    Admin,
    FeeTiers,
    /// Upper bound (bps) that calculate_fee will accept; set by admin (issue #1507).
    MaxFeeBps,
    MerchantVolume(Address),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TierAppliedEvent {
    pub merchant: Address,
    pub volume_usdc: i128,
    pub fee_bps: u32,
    pub fee: i128,
    pub net: i128,
}

#[contract]
pub struct FeeCalculatorContract;

#[contractimpl]
impl FeeCalculatorContract {
    pub fn __constructor(env: Env, admin: Address, tiers: Vec<FeeTier>) {
        Self::validate_tiers(&tiers);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::FeeTiers, &tiers);
    }

    pub fn set_fee_tiers(env: Env, caller: Address, tiers: Vec<FeeTier>) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        Self::validate_tiers(&tiers);
        env.storage().instance().set(&DataKey::FeeTiers, &tiers);
    }

    pub fn set_settlement_caller(env: Env, caller: Address, settlement_caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        env.storage().instance().set(&DataKey::SettlementCaller, &settlement_caller);
    }

    /// Set the maximum fee rate (bps) that calculate_fee will accept (issue #1507).
    /// Any tier that produces a rate above this ceiling will be rejected at calculation time.
    pub fn set_max_fee_bps(env: Env, caller: Address, max_bps: u32) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        if max_bps > 10_000 {
            panic!("max_fee_bps must be <= 10000");
        }
        env.storage().instance().set(&DataKey::MaxFeeBps, &max_bps);
    }

    pub fn get_max_fee_bps(env: Env) -> Option<u32> {
        env.storage().instance().get(&DataKey::MaxFeeBps)
    }

    pub fn get_fee_tiers(env: Env) -> Vec<FeeTier> {
        env.storage()
            .instance()
            .get(&DataKey::FeeTiers)
            .unwrap_or(vec![&env, FeeTier { threshold_usdc: 0, fee_bps: 0 }])
    }

    pub fn calculate_fee(env: Env, merchant: Address, amount: i128) -> (i128, i128, u32) {
        if amount <= 0 {
            panic!("amount must be > 0");
        }
        // Guard against overflow in amount * fee_bps (issue #1508).
        if amount > MAX_AMOUNT_FOR_FEE {
            panic!("amount exceeds maximum allowed for fee computation");
        }

        let volume = Self::update_and_get_volume(&env, &merchant, amount);
        let fee_bps = Self::select_fee_bps(&env, volume);

        // Validate the selected rate against the configured ceiling (issue #1507).
        if let Some(max_bps) = env.storage().instance().get::<DataKey, u32>(&DataKey::MaxFeeBps) {
            if fee_bps > max_bps {
                panic!("selected fee_bps exceeds configured max_fee_bps");
            }
        }

        let fee = amount
            .checked_mul(fee_bps as i128)
            .expect("overflow")
            .checked_div(BPS_DENOMINATOR)
            .expect("division failure");
        let net = amount.checked_sub(fee).expect("underflow");

        env.events().publish(
            ("FEE", "tier_applied"),
            TierAppliedEvent {
                merchant,
                volume_usdc: volume,
                fee_bps,
                fee,
                net,
            },
        );

        (fee, net, fee_bps)
    }

    pub fn get_merchant_volume(env: Env, merchant: Address) -> MerchantVolume {
        let current_ledger = env.ledger().sequence();
        let key = DataKey::MerchantVolume(merchant);
        let mut data = env
            .storage()
            .persistent()
            .get::<DataKey, MerchantVolume>(&key)
            .unwrap_or(MerchantVolume {
                window_start_ledger: current_ledger,
                volume_usdc: 0,
            });

        if current_ledger.saturating_sub(data.window_start_ledger) >= LEDGERS_PER_30_DAYS {
            data.window_start_ledger = current_ledger;
            data.volume_usdc = 0;
            env.storage().persistent().set(&key, &data);
        }

        data
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if &admin != caller {
            panic!("Not admin");
        }
    }

    fn validate_tiers(tiers: &Vec<FeeTier>) {
        if tiers.len() == 0 {
            panic!("tiers must not be empty");
        }

        let mut prev_threshold = -1;
        let mut prev_bps = u32::MAX;

        for i in 0..tiers.len() {
            let tier = tiers.get(i).unwrap();
            if tier.threshold_usdc < 0 {
                panic!("threshold must be >= 0");
            }
            if tier.fee_bps > 10_000 {
                panic!("fee_bps must be <= 10000");
            }
            if tier.threshold_usdc <= prev_threshold {
                panic!("thresholds must be strictly increasing");
            }
            if tier.fee_bps > prev_bps {
                panic!("fee_bps must be non-increasing");
            }
            prev_threshold = tier.threshold_usdc;
            prev_bps = tier.fee_bps;
        }
    }

    fn update_and_get_volume(env: &Env, merchant: &Address, amount: i128) -> i128 {
        let current_ledger = env.ledger().sequence();
        let key = DataKey::MerchantVolume(merchant.clone());
        let mut data = env
            .storage()
            .persistent()
            .get::<DataKey, MerchantVolume>(&key)
            .unwrap_or(MerchantVolume {
                window_start_ledger: current_ledger,
                volume_usdc: 0,
            });

        if current_ledger.saturating_sub(data.window_start_ledger) >= LEDGERS_PER_30_DAYS {
            data.window_start_ledger = current_ledger;
            data.volume_usdc = 0;
        }

        data.volume_usdc = data.volume_usdc.checked_add(amount).expect("volume overflow");
        env.storage().persistent().set(&key, &data);
        data.volume_usdc
    }

    fn select_fee_bps(env: &Env, volume: i128) -> u32 {
        let tiers: Vec<FeeTier> = env.storage().instance().get(&DataKey::FeeTiers).unwrap();
        let mut selected_bps = tiers.get(0).unwrap().fee_bps;

        for i in 0..tiers.len() {
            let tier = tiers.get(i).unwrap();
            if volume >= tier.threshold_usdc {
                selected_bps = tier.fee_bps;
            } else {
                break;
            }
        }

        selected_bps
    }
}
