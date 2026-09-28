#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, vec, Address, BytesN, Env, String, Vec};

const PAGE_SIZE: u32 = 20;

// ── types ────────────────────────────────────────────────────────────────────

/// Immutable on-chain settlement record.
#[contracttype]
#[derive(Clone, Debug)]
pub struct SettlementRecord {
    pub payment_id: BytesN<32>,
    pub merchant: Address,
    pub amount: i128,   // gross amount paid
    pub fee: i128,      // platform fee deducted
    pub net: i128,      // amount remitted to merchant (amount - fee)
    pub timestamp: u64, // Unix timestamp supplied by the caller (NestJS)
    pub fiat_ref: String, // bank / fiat-rail reference ID
}

#[contracttype]
enum DataKey {
    Admin,
    /// SettlementRecord keyed by payment_id
    Settlement(BytesN<32>),
    /// Legacy unbounded merchant index. New records use page-sized index
    /// entries instead so appending does not rewrite a merchant's history.
    MerchantIndex(Address),
    /// Vec<BytesN<32>> — one bounded page of payment IDs for a merchant.
    MerchantPage(Address, u32),
    /// Number of settlement IDs stored for a merchant.
    MerchantCount(Address),
}

// ── events ───────────────────────────────────────────────────────────────────

#[contracttype]
struct SettlementRecordedEvent {
    payment_id: BytesN<32>,
    merchant: Address,
    amount: i128,
    fee: i128,
    net: i128,
    fiat_ref: String,
}

// ── contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct SettlementLedgerContract;

#[contractimpl]
impl SettlementLedgerContract {
    pub fn __constructor(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    /// Transfers authority to write future immutable settlement records.
    /// The current administrator must authorize the rotation.
    pub fn transfer_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    /// Write an immutable settlement record. Admin-only (called by NestJS backend).
    /// Panics if a record for `payment_id` already exists — records are append-only.
    ///
    /// This contract is a trusted-write ledger: it validates that `fee + net`
    /// equals `amount`, but does not query a fee-calculator contract. The
    /// authorized backend is responsible for applying the merchant's configured
    /// fee policy before recording a settlement.
    pub fn record_settlement(
        env: Env,
        caller: Address,
        payment_id: BytesN<32>,
        merchant: Address,
        amount: i128,
        fee: i128,
        net: i128,
        timestamp: u64,
        fiat_ref: String,
    ) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        assert!(amount > 0, "amount must be > 0");
        assert!(fee >= 0, "fee must be >= 0");
        assert!(net >= 0, "net must be >= 0");
        assert!(fee + net == amount, "fee + net must equal amount");

        let key = DataKey::Settlement(payment_id.clone());
        assert!(!env.storage().persistent().has(&key), "settlement already recorded");

        let record = SettlementRecord {
            payment_id: payment_id.clone(),
            merchant: merchant.clone(),
            amount,
            fee,
            net,
            timestamp,
            fiat_ref: fiat_ref.clone(),
        };

        env.storage().persistent().set(&key, &record);

        // Append to one bounded page so recording a settlement never rewrites
        // the merchant's full historical index.
        let count_key = DataKey::MerchantCount(merchant.clone());
        let count: u32 = env
            .storage()
            .persistent()
            .get(&count_key)
            .unwrap_or(0);
        let page = count / PAGE_SIZE;
        let page_key = DataKey::MerchantPage(merchant.clone(), page);
        let mut index: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&page_key)
            .unwrap_or_else(|| vec![&env]);
        index.push_back(payment_id.clone());
        env.storage().persistent().set(&page_key, &index);
        env.storage().persistent().set(&count_key, &(count + 1));

        env.events().publish(
            ("SETTLEMENT_LEDGER", "settlement_recorded"),
            SettlementRecordedEvent {
                payment_id,
                merchant,
                amount,
                fee,
                net,
                fiat_ref,
            },
        );
    }

    /// Fetch a single settlement record by payment_id. Callable by anyone.
    pub fn get_settlement(env: Env, payment_id: BytesN<32>) -> SettlementRecord {
        env.storage()
            .persistent()
            .get(&DataKey::Settlement(payment_id))
            .expect("settlement not found")
    }

    /// Paginated list of settlement records for a merchant.
    /// `page` is 0-indexed; returns up to PAGE_SIZE (20) records per page.
    pub fn list_settlements(env: Env, merchant: Address, page: u32) -> Vec<SettlementRecord> {
        let count_key = DataKey::MerchantCount(merchant.clone());
        let count: u32 = env
            .storage()
            .persistent()
            .get(&count_key)
            .unwrap_or(0);
        if page >= count.saturating_add(PAGE_SIZE - 1) / PAGE_SIZE {
            return vec![&env];
        }

        let page_key = DataKey::MerchantPage(merchant, page);
        let index: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&page_key)
            .unwrap_or_else(|| vec![&env]);
        let mut results: Vec<SettlementRecord> = vec![&env];
        for i in 0..index.len() {
            let pid = index.get(i).unwrap();
            let record: SettlementRecord = env
                .storage()
                .persistent()
                .get(&DataKey::Settlement(pid))
                .unwrap();
            results.push_back(record);
        }
        results
    }

    /// Total number of settlements recorded for a merchant.
    pub fn settlement_count(env: Env, merchant: Address) -> u32 {
        env.storage()
            .persistent()
            .get::<_, u32>(&DataKey::MerchantCount(merchant))
            .unwrap_or(0)
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        assert!(caller == &admin, "not admin");
    }
}
