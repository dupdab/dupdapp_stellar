#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec};

/// Number of ledgers in a ~24h window (5s per ledger).
const ACTIVE_WINDOW_LEDGERS: u32 = 17_280;

/// TTL (in ledgers) applied to `ActiveBucket` persistent entries. Roughly two
/// 24h windows, so the current bucket stays alive while it is being written to
/// and is allowed to naturally expire once it is no longer current.
const ACTIVE_BUCKET_TTL_LEDGERS: u32 = ACTIVE_WINDOW_LEDGERS * 2;

/// Number of daily snapshots retained in the rolling history window.
const STATS_HISTORY_DAYS: u32 = 30;

/// TTL (in ledgers) applied to `StatsSnapshot` persistent entries. Roughly the
/// full retention window plus a margin, so snapshots survive until pruned.
const STATS_SNAPSHOT_TTL_LEDGERS: u32 = ACTIVE_WINDOW_LEDGERS * (STATS_HISTORY_DAYS + 2);

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

/// A point-in-time snapshot of the platform totals, keyed by day index.
#[contracttype]
#[derive(Clone, Debug)]
pub struct StatsSnapshot {
    /// Day index (`ledger_sequence / ACTIVE_WINDOW_LEDGERS`) the snapshot covers.
    pub day: u32,
    pub total_merchants: u32,
    pub total_payments: u32,
    pub total_settled_volume_usd: i128,
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
    /// Daily snapshot of the platform totals, keyed by day index.
    StatsSnapshot(u32),
    /// Ordered list of day indices currently retained in the history window.
    StatsHistoryDays,
    PartnerOk,
}

#[contract]
pub struct PlatformStatsContract;

#[contractimpl]
impl PlatformStatsContract {
    pub fn __constructor(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::TotalMerchants, &0u32);
        env.storage().instance().set(&DataKey::TotalPayments, &0u32);
        env.storage()
            .instance()
            .set(&DataKey::TotalSettledVolumeUsd, &0i128);
        env.storage().instance().set(&DataKey::PartnerOk, &true);
        env.storage()
            .instance()
            .set(&DataKey::StatsHistoryDays, &Vec::<u32>::new(&env));
    }

    /// Admin-only: record a newly registered merchant.
    pub fn record_merchant(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        let prev: u32 = env
            .storage()
            .instance()
            .get(&DataKey::TotalMerchants)
            .unwrap();
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
        let prev: u32 = env
            .storage()
            .instance()
            .get(&DataKey::TotalMerchants)
            .unwrap();
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

        let tp: u32 = env
            .storage()
            .instance()
            .get(&DataKey::TotalPayments)
            .unwrap();
        let next_tp = tp.checked_add(1).expect("counter overflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalPayments, &next_tp);

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

    /// Admin-only: capture a daily snapshot of the current totals.
    ///
    /// Snapshots are keyed by day index (`ledger_sequence /
    /// ACTIVE_WINDOW_LEDGERS`). Re-invoking within the same day overwrites the
    /// existing snapshot so the latest totals for that day are retained. The
    /// rolling history window is capped at `STATS_HISTORY_DAYS`; older entries
    /// are pruned automatically.
    pub fn record_daily_snapshot(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let day = env.ledger().sequence() / ACTIVE_WINDOW_LEDGERS;
        let snapshot = StatsSnapshot {
            day,
            total_merchants: env
                .storage()
                .instance()
                .get(&DataKey::TotalMerchants)
                .unwrap(),
            total_payments: env
                .storage()
                .instance()
                .get(&DataKey::TotalPayments)
                .unwrap(),
            total_settled_volume_usd: env
                .storage()
                .instance()
                .get(&DataKey::TotalSettledVolumeUsd)
                .unwrap(),
        };

        let key = DataKey::StatsSnapshot(day);
        env.storage().persistent().set(&key, &snapshot);
        env.storage().persistent().extend_ttl(
            &key,
            STATS_SNAPSHOT_TTL_LEDGERS,
            STATS_SNAPSHOT_TTL_LEDGERS,
        );

        let mut days: Vec<u32> = env
            .storage()
            .instance()
            .get(&DataKey::StatsHistoryDays)
            .unwrap_or_else(|| Vec::new(&env));
        let mut exists = false;
        for i in 0..days.len() {
            if days.get(i).unwrap() == day {
                exists = true;
                break;
            }
        }
        if !exists {
            days.push_back(day);
        }
        env.storage()
            .instance()
            .set(&DataKey::StatsHistoryDays, &days);

        Self::prune_history(&env);
        env.events()
            .publish(("PLATFORM_STATS", "snapshot_recorded"), (day,));
    }

    /// Admin-only: prune snapshots older than the rolling retention window.
    ///
    /// Removes any snapshot whose day index is more than `STATS_HISTORY_DAYS`
    /// behind the current day, keeping the history bounded.
    pub fn prune_stats_history(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        Self::prune_history(&env);
    }

    /// Return up to `days` most recent daily snapshots, oldest first.
    ///
    /// `days` is clamped to the retention window (`STATS_HISTORY_DAYS`).
    pub fn get_stats_history(env: Env, days: u32) -> Vec<StatsSnapshot> {
        let limit = if days > STATS_HISTORY_DAYS {
            STATS_HISTORY_DAYS
        } else {
            days
        };
        let stored: Vec<u32> = env
            .storage()
            .instance()
            .get(&DataKey::StatsHistoryDays)
            .unwrap_or_else(|| Vec::new(&env));

        let mut out: Vec<StatsSnapshot> = Vec::new(&env);
        let len = stored.len();
        let start = if len > limit { len - limit } else { 0 };
        for i in start..len {
            let day = stored.get(i).unwrap();
            if let Some(snapshot) = env
                .storage()
                .persistent()
                .get::<DataKey, StatsSnapshot>(&DataKey::StatsSnapshot(day))
            {
                out.push_back(snapshot);
            }
        }
        out
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
        let active_payments_24h = current.saturating_add(previous);

        let partner_ok: bool = env
            .storage()
            .instance()
            .get(&DataKey::PartnerOk)
            .unwrap_or(true);

        PlatformStats {
            total_merchants: env
                .storage()
                .instance()
                .get(&DataKey::TotalMerchants)
                .unwrap(),
            total_payments: env
                .storage()
                .instance()
                .get(&DataKey::TotalPayments)
                .unwrap(),
            total_settled_volume_usd: env
                .storage()
                .instance()
                .get(&DataKey::TotalSettledVolumeUsd)
                .unwrap(),
            active_payments_24h,
            health: SystemHealth {
                storage_ok: true,
                stellar_ok: true,
                partner_ok,
            },
        }
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if &admin != caller {
            panic!("not admin");
        }
    }

    /// Drop snapshots outside the rolling retention window and rebuild the
    /// ordered day index accordingly.
    fn prune_history(env: &Env) {
        let current_day = env.ledger().sequence() / ACTIVE_WINDOW_LEDGERS;
        let stored: Vec<u32> = env
            .storage()
            .instance()
            .get(&DataKey::StatsHistoryDays)
            .unwrap_or_else(|| Vec::new(env));

        let mut kept: Vec<u32> = Vec::new(env);
        for i in 0..stored.len() {
            let day = stored.get(i).unwrap();
            if current_day.saturating_sub(day) >= STATS_HISTORY_DAYS {
                env.storage()
                    .persistent()
                    .remove(&DataKey::StatsSnapshot(day));
            } else {
                kept.push_back(day);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::StatsHistoryDays, &kept);
    }
}
