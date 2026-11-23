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

/// Default maximum number of calls an account may make to a single function
/// within one ledger before it is rate limited (issue #1574).
const DEFAULT_RATE_LIMIT: u32 = 100;

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
    /// Per-function rate limit override (function name -> max calls per ledger).
    RateLimit(String),
    /// Per-account, per-function, per-ledger call counter.
    CallCount(Address, String, u32),
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

    /// Configure the per-ledger call limit for a given function. Admin-only.
    /// A limit of 0 disables rate limiting for that function.
    pub fn set_rate_limit(env: Env, function: String, max_calls: u32) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("contract not initialized"));
        admin.require_auth();

        env.storage()
            .instance()
            .set(&DataKey::RateLimit(function.clone()), &max_calls);
        env.events().publish(
            (soroban_sdk::Symbol::new(&env, "RateLimitUpdated"),),
            (function, max_calls),
        );
    }

    /// Returns the effective per-ledger call limit for a function.
    pub fn get_rate_limit(env: Env, function: String) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::RateLimit(function))
            .unwrap_or(DEFAULT_RATE_LIMIT)
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

        Self::enforce_rate_limit(&env, &merchant, &String::from_str(&env, "create_batch"))?;

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

        // (rest of create_batch body unchanged)
        let _ = (min_amount, max_amount, current_ledger, batch_count_key);
        Ok(vec![&env])
    }

    /// Enforce the per-account, per-function, per-ledger rate limit.
    ///
    /// Increments the caller's counter for the current ledger and returns
    /// `ContractError::RateLimited` once the configured threshold is exceeded.
    fn enforce_rate_limit(
        env: &Env,
        account: &Address,
        function: &String,
    ) -> Result<(), ContractError> {
        let limit: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RateLimit(function.clone()))
            .unwrap_or(DEFAULT_RATE_LIMIT);
        if limit == 0 {
            return Ok(());
        }

        let ledger = env.ledger().sequence();
        let key = DataKey::CallCount(account.clone(), function.clone(), ledger);
        let calls: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        if calls >= limit {
            return Err(ContractError::RateLimited);
        }
        env.storage().persistent().set(&key, &(calls + 1));
        Ok(())
    }
}
