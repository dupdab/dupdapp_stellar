#![no_std]

mod test;

use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, vec, xdr::ToXdr, Address, Bytes, BytesN,
    Env, String, Vec,
};

const MAX_BATCH_SIZE: u32 = 100;
const DEFAULT_MAX_BATCH_SIZE: u32 = 20;

/// Maximum number of batches a single merchant may create within one ledger.
const MAX_BATCHES_PER_LEDGER: u32 = 10;

/// ABI version of this contract. Bump on every WASM upgrade.
const CONTRACT_VERSION: u32 = 1;

/// Storage strategy (see docs/storage-strategy.md):
/// - Instance storage holds configuration and small counters (Admin, MinAmount,
///   MaxAmount, MaxBatchSize, Counter, Version). It is shared across the contract
///   instance and its TTL is extended together with the contract, so it must stay small.
/// - Persistent storage holds per-payment records (Payment(id)), which grow with
///   usage and must carry their own TTL extension so they are not archived.
#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    MinAmount,
    MaxAmount,
    MaxBatchSize,
    Counter,
    MerchantBatchCount(Address),
    Version,
    RegistryContract,
    Payment(BytesN<32>),
    /// Platform-wide total payment volume in stroops (issue #1552).
    PlatformTotalVolume,
    /// Platform-wide total payment count (issue #1552).
    PlatformPaymentCount,
}

/// A single payment input in the batch.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PaymentInput {
    /// Amount in stroops (must be > 0).
    pub amount: i128,
    /// Non-empty human-readable memo for the payment.
    pub memo: String,
    /// Optional customer Stellar address.
    pub customer: Option<Address>,
}

/// The result record for each created payment.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PaymentRecord {
    /// Unique payment ID (32-byte hash derived from merchant, the ledger
    /// sequence, and a per-contract counter — see issue #1024).
    pub id: BytesN<32>,
    pub amount: i128,
    pub memo: String,
    pub merchant: Address,
    pub customer: Option<Address>,
}

/// Thin client interface for the MerchantRegistry contract.
#[contractclient(name = "MerchantRegistryClient")]
#[allow(dead_code)]
trait MerchantRegistry {
    fn is_approved(env: Env, merchant: Address) -> bool;
}

#[contract]
pub struct BatchPaymentContract;

#[contractimpl]
impl BatchPaymentContract {
    /// One-time initialization for admin and amount limits.
    pub fn init(env: Env, admin: Address, min_amount: i128, max_amount: i128) -> Result<(), ContractError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }
        if min_amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }
        if min_amount > max_amount {
            return Err(ContractError::InvalidLimits);
        }

        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::MinAmount, &min_amount);
        env.storage()
            .instance()
            .set(&DataKey::MaxAmount, &max_amount);
        env.storage()
            .instance()
            .set(&DataKey::MaxBatchSize, &DEFAULT_MAX_BATCH_SIZE);
        env.storage().instance().set(&DataKey::Counter, &0u64);
        env.storage().instance().set(&DataKey::Version, &CONTRACT_VERSION);
        env.storage()
            .instance()
            .set(&DataKey::PlatformTotalVolume, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::PlatformPaymentCount, &0u64);
        env.events().publish(
            ("BATCH_PAYMENTS", "initialized"),
            (admin, min_amount, max_amount),
        );
    }

    /// Returns the ABI version of this contract.
    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Version)
            .unwrap_or(CONTRACT_VERSION)
    }

    /// Returns the platform-wide total payment volume in stroops.
    pub fn get_platform_total_volume(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::PlatformTotalVolume)
            .unwrap_or(0)
    }

    /// Returns the platform-wide total payment count.
    pub fn get_platform_payment_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::PlatformPaymentCount)
            .unwrap_or(0)
    }

    /// Atomically add a batch's volume and payment count to the platform stats.
    ///
    /// Soroban executes each contract invocation serially against a single
    /// ledger snapshot, so reading the current value and writing the updated
    /// value within this same invocation is atomic — no other invocation can
    /// interleave between the read and the write. This replaces the previous
    /// per-transaction read-modify-write that could drop increments.
    fn apply_platform_stat_delta(env: &Env, volume_delta: i128, count_delta: u64) {
        let total_volume: i128 = env
            .storage()
            .instance()
            .get(&DataKey::PlatformTotalVolume)
            .unwrap_or(0);
        let payment_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PlatformPaymentCount)
            .unwrap_or(0);

        env.storage().instance().set(
            &DataKey::PlatformTotalVolume,
            &total_volume.saturating_add(volume_delta),
        );
        env.storage().instance().set(
            &DataKey::PlatformPaymentCount,
            &payment_count.saturating_add(count_delta),
        );
    }

    /// Update payment amount limits. Admin-only.
    pub fn set_limits(env: Env, min_amount: i128, max_amount: i128) -> Result<(), ContractError> {
        if min_amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }
        if min_amount > max_amount {
            return Err(ContractError::InvalidLimits);
        }

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ContractError::ContractNotInitialized)?;
        admin.require_auth();

        env.storage()
            .instance()
            .set(&DataKey::MinAmount, &min_amount);
        env.storage()
            .instance()
            .set(&DataKey::MaxAmount, &max_amount);
        env.events().publish(
            (soroban_sdk::Symbol::new(&env, "LimitsUpdated"),),
            (min_amount, max_amount),
        );
        Ok(())
    }

    /// Update the maximum batch size. Admin-only.
    pub fn set_max_batch_size(env: Env, new_max: u32) {
        if new_max == 0 || new_max > MAX_BATCH_SIZE {
            panic!("max batch size must be between 1 and {}", MAX_BATCH_SIZE);
        }

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("contract not initialized"));
        admin.require_auth();

        env.storage()
            .instance()
            .set(&DataKey::MaxBatchSize, &new_max);
        env.events().publish(
            (soroban_sdk::Symbol::new(&env, "MaxBatchSizeUpdated"),),
            (new_max,),
        );
    }

    /// Set or clear the merchant registry used to gate batch creation.
    pub fn set_registry(env: Env, registry: Option<Address>) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("contract not initialized"));
        admin.require_auth();
        match registry {
            Some(address) => env
                .storage()
                .instance()
                .set(&DataKey::RegistryContract, &address),
            None => env.storage().instance().remove(&DataKey::RegistryContract),
        }
    }

    /// Create up to max_batch_size payments atomically in a single contract invocation.
    ///
    /// Validates every input before any state is written — if any item is
    /// invalid the entire batch reverts. Emits a `PaymentCreated` event for
    /// each created payment, matching the NestJS service event log.
    ///
    /// Returns a `Vec<BytesN<32>>` of the created payment IDs.
    pub fn create_batch(
        env: Env,
        merchant: Address,
        payments: Vec<PaymentInput>,
    ) -> Result<Vec<BytesN<32>>, ContractError> {
        merchant.require_auth();

        let current_ledger = env.ledger().sequence();
        let batch_count_key = DataKey::MerchantBatchCount(merchant.clone());
        let (count_ledger, batches_in_ledger): (u32, u32) = env
            .storage()
            .persistent()
            .get(&batch_count_key)
            .unwrap_or((current_ledger, 0));
        if count_ledger == current_ledger && batches_in_ledger >= MAX_BATCHES_PER_LEDGER {
            panic!("merchant exceeded maximum batches per ledger");
        }

        // Gate on merchant registry when one is configured.
        if let Some(registry_addr) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::RegistryContract)
        {
            let registry_client = MerchantRegistryClient::new(&env, &registry_addr);
            if !registry_client.is_approved(&merchant) {
                panic!("merchant is not approved in the registry");
            }
        }

        let min_amount: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MinAmount)
            .ok_or(ContractError::ContractNotInitialized)?;
        let max_amount: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MaxAmount)
            .unwrap_or_else(|| panic!("contract not initialized"));
        let max_batch_size: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MaxBatchSize)
            .unwrap_or(DEFAULT_MAX_BATCH_SIZE);

        let count = payments.len();
        if count == 0 {
            return Err(ContractError::BatchEmpty);
        }
        if count > max_batch_size {
            return Err(ContractError::BatchTooLarge);
        }

        // Validate every input before writing any state (atomicity).
        let mut volume_delta: i128 = 0;
        for i in 0..count {
            let payment = payments.get(i).unwrap();
            if payment.amount <= 0 {
                return Err(ContractError::InvalidAmount);
            }
            if payment.amount < min_amount || payment.amount > max_amount {
                return Err(ContractError::AmountOutOfRange);
            }
            if payment.memo.len() == 0 {
                return Err(ContractError::EmptyMemo);
            }
            volume_delta = volume_delta.saturating_add(payment.amount);
        }

        // Apply the platform stat delta atomically within this invocation.
        Self::apply_platform_stat_delta(&env, volume_delta, count as u64);

        // ... existing per-payment creation logic continues below ...
        let mut ids: Vec<BytesN<32>> = vec![&env];
            }
            if payment.memo.len() == 0 {
                return Err(ContractError::EmptyMemo);
            }
            volume_delta = volume_delta.saturating_add(payment.amount);
        }

        // Apply the platform stat delta atomically within this invocation.
        Self::apply_platform_stat_delta(&env, volume_delta, count as u64);

        // ... existing per-payment creation logic continues below ...
        let mut ids: Vec<BytesN<32>> = vec![&env];
        let mut counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::Counter)
            .unwrap_or(0);
        let mut ids: Vec<BytesN<32>> = vec![&env];

        for i in 0..count {
            let payment = payments.get(i).unwrap();
            counter += 1;

            // Derive a deterministic 32-byte payment ID from merchant, ledger
            // sequence, and the per-contract counter.
            let mut preimage: Bytes = merchant.clone().to_xdr(&env);
            preimage.extend_from_array(&current_ledger.to_be_bytes());
            preimage.extend_from_array(&counter.to_be_bytes());
            let id: BytesN<32> = env.crypto().sha256(&preimage).into();
            let record = PaymentRecord {
                id: id.clone(),
                amount: payment.amount,
                memo: payment.memo.clone(),
                merchant: merchant.clone(),
                customer: payment.customer.clone(),
            };
            env.storage()
                .persistent()
                .set(&DataKey::Payment(id.clone()), &record);
            env.events().publish(
                (soroban_sdk::Symbol::new(&env, "PaymentCreated"),),
                (id.clone(), payment.amount, payment.memo.clone()),
            );

            ids.push_back(id);
        }
        env.storage().instance().set(&DataKey::Counter, &counter);
        env.storage()
            .persistent()
            .set(&batch_count_key, &(current_ledger, batches_in_ledger + 1));

            env.events().publish(
                (soroban_sdk::Symbol::new(&env, "PaymentCreated"),),
                (id.clone(), payment.amount, payment.memo.clone()),
            );

            ids.push_back(id);
        }

        // Emit a batch-level event carrying every created payment ID so the
        // backend can correlate the batch to its individual payments (#1529).
        env.events().publish(
            (soroban_sdk::Symbol::new(&env, "BatchCreated"),),
            (merchant.clone(), ids.clone()),
        );

        Ok(ids)
    }
}

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractError {
    AlreadyInitialized = 1,
    ContractNotInitialized = 2,
    InvalidAmount = 3,
    InvalidLimits = 4,
    BatchEmpty = 5,
    BatchTooLarge = 6,
    EmptyMemo = 7,
}
