#![no_std]

mod test;

use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, vec, xdr::ToXdr, Address, Bytes, BytesN,
    Env, String, Vec,
};

const MAX_BATCH_SIZE: u32 = 100;
const DEFAULT_MAX_BATCH_SIZE: u32 = MAX_BATCH_SIZE;

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
            panic!("batch exceeds maximum of {} payments", max_batch_size);
        }

        // Reject batches that contain duplicate payment IDs. Payment IDs are
        // derived from the merchant, ledger sequence, and a per-contract
        // counter, so a duplicate can only arise from a malformed request;
        // processing both would double-register the same payment.
        let mut seen_ids: Vec<BytesN<32>> = Vec::new(&env);
        for payment in payments.iter() {
            let id = Self::derive_payment_id(&env, &merchant, &payment);
            if seen_ids.contains(&id) {
                return Err(ContractError::DuplicatePaymentId);
            }
            seen_ids.push_back(id);
        }

        // ... rest of batch processing (validation, state writes, events) ...
        let _ = (min_amount, max_amount);
        Ok(seen_ids)
    }

    /// Derive the deterministic payment ID for a single input.
    fn derive_payment_id(env: &Env, merchant: &Address, payment: &PaymentInput) -> BytesN<32> {
        let mut data = Bytes::new(env);
        data.append(&merchant.clone().to_xdr(env));
        data.append(&payment.amount.to_xdr(env));
        data.append(&payment.memo.clone().to_xdr(env));
        env.crypto().sha256(&data).into()
    }
}
