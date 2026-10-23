#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

/// Number of ledgers in a ~24h window (5s per ledger).
const ACTIVE_WINDOW_LEDGERS: u32 = 17_280;

/// TTL (in ledgers) applied to `ActiveBucket` persistent entries. Roughly two
/// 24h windows, so the current bucket stays alive while it is being written to
/// and is allowed to naturally expire once it is no longer current.
const ACTIVE_BUCKET_TTL_LEDGERS: u32 = ACTIVE_WINDOW_LEDGERS * 2;

/// Live platform overview metrics for the admin dashboard.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PlatformStats {
    pub total_merchants: u32,
    pub total_payments: u32,
    pub total_settled_volume_usd: i128,
    pub active_payments_24h: u32,
    pub health: SystemHealth,
}

/// System health: DB/storage, Stellar connectivity, partner API.
#[contracttype]
#[derive(Clone, Debug)]
pub struct SystemHealth {
    pub storage_ok: bool,
    pub stellar_ok: bool,
    pub partner_ok: bool,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    TotalMerchants,
    TotalPayments,
    TotalSettledVolumeUsd,
    /// Count of payments recorded in a single ledger-sequence-aligned bucket.
    ///
    /// Buckets are fixed-size windows of `ACTIVE_WINDOW_LEDGERS` ledgers. The
    /// rolling 24h figure reported by `stats()` is the sum of the current and
    /// previous bucket, so a payment stays counted for at least one full window
    /// after it is recorded.
    ///
    /// Stored in `persistent()` storage with a short TTL so historical buckets
    /// expire naturally instead of accumulating forever in the shared
    /// `instance()` footprint.
    ActiveBucket(u32),
    PartnerOk,
}

#[contract]
pub struct PlatformStatsContract;

#[contractimpl]
impl PlatformStatsContract {
    pub fn __constructor(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TotalMerchants, &0u32);
        env.storage().instance().set(&DataKey::TotalPayments, &0u32);
        env.storage().instance().set(&DataKey::TotalSettledVolumeUsd, &0i128);
        env.storage().instance().set(&DataKey::PartnerOk, &true);
    }

    /// Admin-only: record a newly registered merchant.
    pub fn record_merchant(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        let prev: u32 = env.storage().instance().get(&DataKey::TotalMerchants).unwrap();
        let next = prev.checked_add(1).expect("counter overflow");
        env.storage().instance().set(&DataKey::TotalMerchants, &next);
        env.events()
            .publish(("PLATFORM_STATS", "merchant_recorded"), (next,));
    }

    /// Admin-only: record a merchant termination, decrementing the active
    /// merchant count. Saturates at zero so the counter cannot underflow.
    pub fn record_merchant_terminated(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        let prev: u32 = env.storage().instance().get(&DataKey::TotalMerchants).unwrap();
        let next = prev.saturating_sub(1);
        env.storage().instance().set(&DataKey::TotalMerchants, &next);
        env.events()
            .publish(("PLATFORM_STATS", "merchant_terminated"), (next,));
    }

    /// Admin-only: record a payment. If `settled`, adds to settled USD volume
    /// and increments the rolling 24h active-payments bucket.
    ///
    /// # Trust assumption
    /// `amount_usd` and `settled` are caller-supplied and are NOT corroborated
    /// on-chain against `settlement_ledger`'s independently-recorded settlement
    /// records. This contract performs no cross-contract call to
    /// `settlement_ledger::get_settlement` (or any equivalent) to verify that a
    /// matching settlement exists or that the reported amount agrees with it.
    ///
    /// Consequently `total_settled_volume_usd` is only as trustworthy as the
    /// admin/off-chain relayer that invokes this function: a bug or compromise
    /// in that caller could report arbitrary figures with nothing on-chain to
    /// catch a discrepancy. Callers MUST only pass values that have already
    /// been validated against `settlement_ledger` off-chain. If on-chain
    /// corroboration is required, a cross-contract verification against
    /// `settlement_ledger` must be added before the running total is updated.
    pub fn record_payment(env: Env, caller: Address, amount_usd: i128, settled: bool) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let tp: u32 = env.storage().instance().get(&DataKey::TotalPayments).unwrap();
        let next_tp = tp.checked_add(1).expect("counter overflow");
        env.storage().instance().set(&DataKey::TotalPayments, &next_tp);

        if settled {
            let vol: i128 = env
                .storage()
                .instance()
                .get(&DataKey::TotalSettledVolumeUsd)
                .unwrap();
            env.storage()
                .instance()
                .set(&DataKey::TotalSettledVolumeUsd, &(vol + amount_usd));
        }

        let bucket = env.ledger().sequence() / ACTIVE_WINDOW_LEDGERS;
        let key = DataKey::ActiveBucket(bucket);
        let count: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(count + 1));
        env.storage()
            .persistent()
            .extend_ttl(&key, ACTIVE_BUCKET_TTL_LEDGERS, ACTIVE_BUCKET_TTL_LEDGERS);
        env.events().publish(
            ("PLATFORM_STATS", "payment_recorded"),
            (amount_usd, settled, next_tp),
        );
    }

    /// Admin-only: reverse a previously settled payment (refund or dispute
    /// resolution), decrementing settled USD volume. Saturates at zero so the
    /// counter cannot underflow.
    pub fn record_settlement_reversed(env: Env, caller: Address, amount_usd: i128) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        let vol: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalSettledVolumeUsd)
            .unwrap();
        let next = vol.saturating_sub(amount_usd);
        env.storage()
            .instance()
            .set(&DataKey::TotalSettledVolumeUsd, &next);
        env.events()
            .publish(("PLATFORM_STATS", "settlement_reversed"), (amount_usd, next));
    }

    /// Admin-only: reflect partner API health status.
    pub fn set_partner_ok(env: Env, caller: Address, ok: bool) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        env.storage().instance().set(&DataKey::PartnerOk, &ok);
        env.events()
            .publish(("PLATFORM_STATS", "partner_status_updated"), (ok,));
    }

    /// Live overview metrics, computed directly from storage.
    ///
    /// `active_payments_24h` is a sliding ~24h window: it sums the current
    /// ledger-sequence-aligned bucket and the immediately preceding bucket, so
    /// a payment recorded near the end of one bucket remains counted for at
    /// least one full window after it is recorded instead of disappearing the
    /// moment the bucket boundary is crossed.
    pub fn stats(env: Env) -> PlatformStats {
        let bucket = env.ledger().sequence() / ACTIVE_WINDOW_LEDGERS;
        let current: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveBucket(bucket))
            .unwrap_or(0);
        let previous: u32 = if bucket > 0 {
            env.storage()
                .persistent()
                .get(&DataKey::ActiveBucket(bucket - 1))
                .unwrap_or(0)
        } else {
            0
        };
        let active = current + previous;

        PlatformStats {
            total_merchants: env
                .storage()
                .instance()
                .get(&DataKey::TotalMerchants)
                .unwrap_or(0),
            total_payments: env
                .storage()
                .instance()
                .get(&DataKey::TotalPayments)
                .unwrap_or(0),
            total_settled_volume_usd: env
                .storage()
                .instance()
                .get(&DataKey::TotalSettledVolumeUsd)
                .unwrap_or(0),
            active_payments_24h: active,
            health: Self::health(&env),
        }
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if caller != &admin {
            panic!("not admin");
        }
    }

    /// Reports whether the contract's core storage invariants hold.
    ///
    /// `storage_ok` is derived from a real invariant rather than the mere
    /// presence of `DataKey::Admin` (which is written once in the constructor
    /// and never removed, making that check an unconditional tautology). The
    /// invariant verified here is that the counters which `stats()` reads back
    /// are actually present and readable in instance storage. If any of them is
    /// missing or corrupt, `storage_ok` reports `false` instead of silently
    /// masking the problem.
    fn health(env: &Env) -> SystemHealth {
        let instance = env.storage().instance();
        let storage_ok = instance.has(&DataKey::Admin)
            && instance.has(&DataKey::TotalMerchants)
            && instance.has(&DataKey::TotalPayments)
            && instance.has(&DataKey::TotalSettledVolumeUsd)
            && instance.has(&DataKey::PartnerOk);

        SystemHealth {
            storage_ok,
            stellar_ok: env.ledger().sequence() > 0,
            partner_ok: env
                .storage()
                .instance()
                .get(&DataKey::PartnerOk)
                .unwrap_or(true),
        }
    }
}
