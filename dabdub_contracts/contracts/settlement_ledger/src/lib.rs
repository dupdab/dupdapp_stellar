#![no_std]

mod test;

use fee_calculator::FeeCalculatorContractClient;
use soroban_sdk::{contract, contractimpl, contracttype, vec, Address, BytesN, Env, String, Vec};

const PAGE_SIZE: u32 = 20;
/// Archived records have their TTL bumped to ~30 days once their remaining TTL
/// drops below ~1 day (ledgers close roughly every 5 seconds).
const ARCHIVE_TTL_THRESHOLD: u32 = 17_280;
const ARCHIVE_TTL_EXTEND_TO: u32 = 518_400;

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
    pub currency: String, // ISO 4217 currency code (e.g. "NGN")
}

/// An append-only correction marker. The original settlement remains readable
/// and auditable; consumers must exclude voided records from financial totals.
#[contracttype]
#[derive(Clone, Debug)]
pub struct SettlementVoid {
    pub payment_id: BytesN<32>,
    pub reason: String,
    pub voided_at: u64,
}

/// An append-only reversal (chargeback) marker referencing the original
/// settlement. The original record stays readable; consumers must exclude
/// reversed settlements from financial totals.
#[contracttype]
#[derive(Clone, Debug)]
pub struct SettlementReversal {
    pub original_payment_id: BytesN<32>,
    pub merchant: Address,
    pub amount: i128,
    pub reason: String,
    pub reversed_at: u64,
}

#[contracttype]
enum DataKey {
    Admin,
    FeeCalculatorContract,
    /// SettlementRecord keyed by payment_id
    Settlement(BytesN<32>),
    /// Legacy unbounded merchant index. New records use page-sized index
    /// entries instead so appending does not rewrite a merchant's history.
    MerchantIndex(Address),
    /// Vec<BytesN<32>> — one bounded page of payment IDs for a merchant.
    MerchantPage(Address, u32),
    /// Number of settlement IDs stored for a merchant.
    MerchantCount(Address),
    AllCount,
    /// Vec<BytesN<32>> — platform-wide ordered settlement IDs.
    AllIndex,
    /// A void marker keyed by the original payment ID; never overwrites it.
    Void(BytesN<32>),
    /// A reversal marker keyed by the original payment ID; never overwrites it.
    Reversal(BytesN<32>),
    /// Vec<BytesN<32>> — ordered reversal IDs for a merchant.
    MerchantReversalIndex(Address),
    MerchantReversalCount(Address),
    /// Ledger sequence at which a settlement was recorded (used for archival).
    RecordedLedger(BytesN<32>),
    /// SettlementRecord moved out of `Settlement` by `archive_settlements`.
    Archived(BytesN<32>),
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

#[contracttype]
struct SettlementReversedEvent {
    payment_id: BytesN<32>,
    merchant: Address,
    amount: i128,
    reason: String,
}

#[contracttype]
struct SettlementArchivedEvent {
    payment_id: BytesN<32>,
    merchant: Address,
    archived_at_ledger: u32,
}

// ── contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct SettlementLedgerContract;

#[contractimpl]
impl SettlementLedgerContract {
    pub fn __constructor(env: Env, admin: Address, fee_calculator_contract: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::FeeCalculatorContract, &fee_calculator_contract);
    }

    /// Transfers authority to write future immutable settlement records.
    /// The current administrator must authorize the rotation.
    pub fn transfer_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    /// Looks up a settlement record by payment_id.
    /// Falls back to the archive when the record was moved by
    /// `archive_settlements`.
    fn load_record(env: &Env, payment_id: &BytesN<32>) -> Option<SettlementRecord> {
        let live: Option<SettlementRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Settlement(payment_id.clone()));
        if live.is_some() {
            return live;
        }
        env.storage()
            .persistent()
            .get(&DataKey::Archived(payment_id.clone()))
    }

    /// Admin-only. Archives up to `limit` settlements recorded at least
    /// `older_than_ledgers` ledgers ago by moving each one from its primary key
    /// to a secondary `Archived` key with an extended TTL, and emits a
    /// `settlement_archived` event per record. Archived records stay readable
    /// through `get_settlement`, but can no longer be voided or reversed.
    /// Settlements recorded before ledger tracking existed are skipped.
    /// Returns the number of records archived.
    pub fn archive_settlements(
        env: Env,
        caller: Address,
        older_than_ledgers: u32,
        limit: u32,
    ) -> u32 {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let current = env.ledger().sequence();
        let ids: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&DataKey::AllIndex)
            .unwrap_or_else(|| vec![&env]);
        let mut archived: u32 = 0;
        for i in 0..ids.len() {
            if archived >= limit {
                break;
            }
            let id = ids.get(i).unwrap();
            let recorded: Option<u32> = env
                .storage()
                .persistent()
                .get(&DataKey::RecordedLedger(id.clone()));
            let recorded = match recorded {
                Some(ledger) => ledger,
                None => continue,
            };
            if current.saturating_sub(recorded) < older_than_ledgers {
                continue;
            }
            let live_key = DataKey::Settlement(id.clone());
            let record: SettlementRecord = match env.storage().persistent().get(&live_key) {
                Some(record) => record,
                None => continue, // already archived
            };

            let archive_key = DataKey::Archived(id.clone());
            env.storage().persistent().set(&archive_key, &record);
            env.storage().persistent().extend_ttl(
                &archive_key,
                ARCHIVE_TTL_THRESHOLD,
                ARCHIVE_TTL_EXTEND_TO,
            );
            env.storage().persistent().remove(&live_key);
            archived += 1;

            env.events().publish(
                ("SETTLEMENT_LEDGER", "settlement_archived"),
                SettlementArchivedEvent {
                    payment_id: id,
                    merchant: record.merchant,
                    archived_at_ledger: current,
                },
            );
        }
        archived
    }

    /// True if the settlement has been moved to the archive.
    pub fn is_archived(env: Env, payment_id: BytesN<32>) -> bool {
        env.storage().persistent().has(&DataKey::Archived(payment_id))
    }

    /// Write an immutable settlement record. Admin-only (called by NestJS backend).
    /// Idempotent: `payment_id` is the idempotency key. If a record for it
    /// already exists, the existing record is returned unchanged and nothing is
    /// written or emitted, so backend retries never create duplicates.
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
        currency: String,
    ) -> SettlementRecord {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Idempotent replay: return the stored entry for a duplicate payment_id.
        if let Some(existing) = Self::load_record(&env, &payment_id) {
            return existing;
        }

        assert!(amount > 0, "amount must be > 0");
        assert!(fee >= 0, "fee must be >= 0");
        assert!(net >= 0, "net must be >= 0");
        assert!(fee + net == amount, "fee + net must equal amount");

        let fee_calculator_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCalculatorContract)
            .expect("fee calculator not configured");
        let fee_calculator = FeeCalculatorContractClient::new(&env, &fee_calculator_contract);
        let (calculated_fee, calculated_net, _) = fee_calculator.calculate_fee(
            &env.current_contract_address(),
            &merchant,
            &amount,
        );
        assert!(fee == calculated_fee, "fee does not match fee calculator");
        assert!(net == calculated_net, "net does not match fee calculator");

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
            currency,
        };

        env.storage().persistent().set(&key, &record);
        env.storage()
            .persistent()
            .set(&DataKey::RecordedLedger(payment_id.clone()), &env.ledger().sequence());

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

        let mut all: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&DataKey::AllIndex)
            .unwrap_or_else(|| vec![&env]);
        all.push_back(payment_id.clone());
        env.storage().persistent().set(&DataKey::AllIndex, &all);
        env.storage()
            .instance()
            .set(&DataKey::AllCount, &(all.len()));

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

        record
    }

    /// Fetch a single settlement record by payment_id. Callable by anyone.
    pub fn get_settlement(env: Env, payment_id: BytesN<32>) -> SettlementRecord {
        Self::load_record(&env, &payment_id).expect("settlement not found")
    }

    /// Marks a settlement as void without mutating or deleting the original
    /// immutable record. A corrected settlement must be recorded under a new
    /// payment ID and can reference this one in its fiat reference.
    pub fn void_settlement(env: Env, caller: Address, payment_id: BytesN<32>, reason: String) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        assert!(env.storage().persistent().has(&DataKey::Settlement(payment_id.clone())), "settlement not found");
        let key = DataKey::Void(payment_id.clone());
        assert!(!env.storage().persistent().has(&key), "settlement already voided");
        env.storage().persistent().set(&key, &SettlementVoid { payment_id, reason, voided_at: env.ledger().timestamp() });
    }

    /// Returns the void marker for a settlement, if it was corrected.
    pub fn get_settlement_void(env: Env, payment_id: BytesN<32>) -> Option<SettlementVoid> {
        env.storage().persistent().get(&DataKey::Void(payment_id))
    }

    /// Records a reversal (chargeback) against an existing settlement without
    /// mutating or deleting the original immutable record. Admin-only.
    /// Panics if the settlement does not exist or has already been reversed.
    pub fn record_reversal(
        env: Env,
        caller: Address,
        original_payment_id: BytesN<32>,
        reason: String,
    ) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let record: SettlementRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Settlement(original_payment_id.clone()))
            .expect("settlement not found");

        let key = DataKey::Reversal(original_payment_id.clone());
        assert!(!env.storage().persistent().has(&key), "settlement already reversed");

        let reversal = SettlementReversal {
            original_payment_id: original_payment_id.clone(),
            merchant: record.merchant.clone(),
            amount: record.amount,
            reason: reason.clone(),
            reversed_at: env.ledger().timestamp(),
        };
        env.storage().persistent().set(&key, &reversal);

        // Update the merchant's reversal index so reversals are discoverable
        // per merchant without rewriting the settlement index.
        let idx_key = DataKey::MerchantReversalIndex(record.merchant.clone());
        let mut idx: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&idx_key)
            .unwrap_or_else(|| vec![&env]);
        idx.push_back(original_payment_id.clone());
        env.storage().persistent().set(&idx_key, &idx);
        let reversal_count_key = DataKey::MerchantReversalCount(record.merchant.clone());
        let reversal_count: u32 = env
            .storage()
            .persistent()
            .get(&reversal_count_key)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&reversal_count_key, &(reversal_count + 1));

        env.events().publish(
            ("SETTLEMENT_LEDGER", "settlement_reversed"),
            SettlementReversedEvent {
                payment_id: original_payment_id,
                merchant: record.merchant,
                amount: record.amount,
                reason,
            },
        );
    }

    /// Returns the reversal marker for a settlement, if it was reversed.
    pub fn get_settlement_reversal(env: Env, payment_id: BytesN<32>) -> Option<SettlementReversal> {
        env.storage().persistent().get(&DataKey::Reversal(payment_id))
    }

    pub fn get_reversal(env: Env, payment_id: BytesN<32>) -> Option<SettlementReversal> {
        Self::get_settlement_reversal(env, payment_id)
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
            let record: SettlementRecord = Self::load_record(&env, &pid).unwrap();
            results.push_back(record);
        }
        results
    }

    /// Total number of settlements recorded for a merchant.
    pub fn settlement_count(env: Env, merchant: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::MerchantCount(merchant))
            .unwrap_or(0)
    }

    pub fn reversal_count(env: Env, merchant: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::MerchantReversalCount(merchant))
            .unwrap_or(0)
    }

    pub fn list_reversals(env: Env, merchant: Address, page: u32) -> Vec<SettlementReversal> {
        let index: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantReversalIndex(merchant))
            .unwrap_or_else(|| vec![&env]);
        let start = page.saturating_mul(PAGE_SIZE);
        let mut results = vec![&env];
        for position in start..index.len().min(start.saturating_add(PAGE_SIZE)) {
            let payment_id = index.get(position).unwrap();
            if let Some(reversal) = env.storage().persistent().get(&DataKey::Reversal(payment_id)) {
                results.push_back(reversal);
            }
        }
        results
    }

    pub fn total_settlement_count(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::AllCount).unwrap_or(0)
    }

    pub fn list_all_settlements(env: Env, page: u32) -> Vec<SettlementRecord> {
        let ids: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&DataKey::AllIndex)
            .unwrap_or_else(|| vec![&env]);
        let start = page.saturating_mul(PAGE_SIZE);
        let mut results = vec![&env];
        for position in start..ids.len().min(start.saturating_add(PAGE_SIZE)) {
            let payment_id = ids.get(position).unwrap();
            results.push_back(Self::load_record(&env, &payment_id).unwrap());
        }
        results
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        assert!(caller == &admin, "caller is not admin");
    }
}
