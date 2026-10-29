#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec};

const WINDOW_SECONDS: u64 = 30 * 24 * 60 * 60;
const BPS_DENOMINATOR: i128 = 10_000;
const DEFAULT_MIN_FEE_BPS: u32 = 10;

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FeeTier {
    pub threshold_usdc: i128,
    pub fee_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct MerchantVolume {
    pub window_start_timestamp: u64,
    pub volume_usdc: i128,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    SettlementCaller,
    Admin,
    FeeTiers,
    MinFeeStroops,
    MinFeeBps,
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

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FeeTiersUpdatedEvent {
    pub admin: Address,
    pub tiers: Vec<FeeTier>,
}

#[contract]
pub struct FeeCalculatorContract;

#[contractimpl]
impl FeeCalculatorContract {
    pub fn __constructor(env: Env, admin: Address, tiers: Vec<FeeTier>, min_fee_bps: u32) {
        Self::validate_tiers(&tiers);
        if min_fee_bps > 10_000 {
            panic!("min_fee_bps must be <= 10000");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::FeeTiers, &tiers);
        env.storage()
            .instance()
            .set(&DataKey::MinFeeStroops, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::MinFeeBps, &min_fee_bps);
    }

    pub fn set_fee_tiers(env: Env, caller: Address, tiers: Vec<FeeTier>) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        Self::validate_tiers(&tiers);
        env.storage().instance().set(&DataKey::FeeTiers, &tiers);

        env.events().publish(
            ("FEE", "tiers_updated"),
            FeeTiersUpdatedEvent {
                admin: caller,
                tiers,
            },
        );
    }

    pub fn set_settlement_caller(env: Env, caller: Address, settlement_caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        env.storage()
            .instance()
            .set(&DataKey::SettlementCaller, &settlement_caller);
        env.events().publish(
            ("FEE", "settlement_caller_updated"),
            (settlement_caller,),
        );
    }

    pub fn get_settlement_caller(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::SettlementCaller)
    }

    pub fn set_min_fee(ctx: Env, caller: Address, min_fee_bps: u32) {
        Self::set_min_fee_bps(ctx, caller, min_fee_bps);
    }

    pub fn get_min_fee(env: Env) -> u32 {
        Self::get_min_fee_bps(env)
    }

    pub fn set_min_fee_stroops(env: Env, caller: Address, min_fee_stroops: i128) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        if min_fee_stroops < 0 {
            panic!("min_fee_stroops must be >= 0");
        }
        env.storage()
            .instance()
            .set(&DataKey::MinFeeStroops, &min_fee_stroops);
        env.events()
            .publish(("FEE", "min_fee_stroops_updated"), (min_fee_stroops,));
    }

    pub fn get_min_fee_stroops(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::MinFeeStroops)
            .unwrap_or(0)
    }

    pub fn set_min_fee_bps(env: Env, caller: Address, min_fee_bps: u32) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        if min_fee_bps > 10_000 {
            panic!("min_fee_bps must be <= 10000");
        }
        env.storage()
            .instance()
            .set(&DataKey::MinFeeBps, &min_fee_bps);
        env.events()
            .publish(("FEE", "min_fee_bps_updated"), (min_fee_bps,));
    }

    pub fn get_min_fee_bps(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MinFeeBps)
            .unwrap_or(DEFAULT_MIN_FEE_BPS)
    }

    pub fn set_settlement_caller(env: Env, caller: Address, settlement_caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        env.storage().instance().set(&DataKey::SettlementCaller, &settlement_caller);
    }

    pub fn get_fee_tiers(env: Env) -> Vec<FeeTier> {
        env.storage()
            .instance()
            .get(&DataKey::FeeTiers)
            .expect("tiers not initialized")
    }

    pub fn calculate_fee(
        env: Env,
        caller: Address,
        merchant: Address,
        amount: i128,
    ) -> (i128, i128, u32) {
        caller.require_auth();
        Self::require_authorized_caller(&env, &caller);

        if amount <= 0 {
            panic!("amount must be > 0");
        }

        let volume = Self::update_and_get_volume(&env, &merchant, amount);
        let min_fee_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MinFeeBps)
            .unwrap_or(DEFAULT_MIN_FEE_BPS);
        let mut fee_bps = Self::select_fee_bps(&env, volume).max(min_fee_bps);

        let mut fee = amount
            .checked_mul(fee_bps as i128)
            .expect("overflow")
            .checked_div(BPS_DENOMINATOR)
            .expect("division failure");

        let min_fee_from_bps = amount
            .checked_mul(min_fee_bps as i128)
            .expect("overflow")
            .checked_div(BPS_DENOMINATOR)
            .expect("division failure");
        if fee < min_fee_from_bps {
            fee = min_fee_from_bps;
        }

        let min_fee_stroops: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MinFeeStroops)
            .unwrap_or(0);
        if fee < min_fee_stroops {
            fee = min_fee_stroops;
        }
        if fee > amount {
            fee = amount;
        }

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

    /// Returns the merchant's current rolling volume for the last 30 days.
    pub fn get_merchant_volume(env: Env, merchant: Address) -> i128 {
        let current_timestamp = env.ledger().timestamp();
        let key = DataKey::MerchantVolume(merchant);
        let data = env
            .storage()
            .persistent()
            .get::<DataKey, MerchantVolume>(&key)
            .unwrap_or(MerchantVolume {
                window_start_timestamp: current_timestamp,
                volume_usdc: 0,
            });

        if current_timestamp
            .saturating_sub(data.window_start_timestamp)
            >= WINDOW_SECONDS
        {
            0
        } else {
            data.volume_usdc
        }
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if &admin != caller {
            panic!("Not admin");
        }
    }

    fn require_authorized_caller(env: &Env, caller: &Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if caller == &admin {
            return;
        }
        if let Some(settlement_caller) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::SettlementCaller)
        {
            if caller == &settlement_caller {
                return;
            }
        }
        panic!("Not authorized");
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

    fn select_fee_bps(env: &Env, volume: i128) -> u32 {
        let tiers: Vec<FeeTier> = env
            .storage()
            .instance()
            .get(&DataKey::FeeTiers)
            .expect("tiers not initialized");

        let len = tiers.len();
        let mut lo: u32 = 0;
        let mut hi: u32 = len;

        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let tier = tiers.get(mid).unwrap();
            if tier.threshold_usdc <= volume {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }

        let idx = if lo == 0 { 0 } else { lo - 1 };
        tiers.get(idx).unwrap().fee_bps
    }

    fn update_and_get_volume(env: &Env, merchant: &Address, amount: i128) -> i128 {
        let current_timestamp = env.ledger().timestamp();
        let key = DataKey::MerchantVolume(merchant.clone());
        let mut data = env
            .storage()
            .persistent()
            .get::<DataKey, MerchantVolume>(&key)
            .unwrap_or(MerchantVolume {
                window_start_timestamp: current_timestamp,
                volume_usdc: 0,
            });

        if current_timestamp
            .saturating_sub(data.window_start_timestamp)
            >= WINDOW_SECONDS
        {
            data.window_start_timestamp = current_timestamp;
            data.volume_usdc = 0;
        }

        data.volume_usdc = data.volume_usdc.checked_add(amount).expect("overflow");
        env.storage().persistent().set(&key, &data);
        data.volume_usdc
    }
}
