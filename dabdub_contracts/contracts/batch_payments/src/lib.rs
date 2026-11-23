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
    /// Amount in stroops (must be > 0).
    pub amount: i128,
    /// Non-empty human-readable memo for the payment.
    pub memo: String,
    /// Merchant that owns the payment.
    pub merchant: Address,
    /// Optional customer Stellar address.
    pub customer: Option<Address>,
}

/// Thin client interface for the MerchantRegistry contract.
#[contractclient(name = "MerchantRegistryClient")]
#[allow(dead_code)]
trait MerchantRegistry {
    /// Returns `true` when `merchant` is approved in the registry.
    fn is_approved(env: Env, merchant: Address) -> bool;
}

#[contract]
pub struct BatchPaymentContract;

#[contractimpl]
impl BatchPaymentContract {
    /// One-time initialization for admin and amount limits.
    ///
    /// # Parameters
    /// - `admin`: Address that will own admin-only operations. Must authorize this call.
    /// - `min_amount`: Minimum payment amount in stroops. Must be `> 0`.
    /// - `max_amount`: Maximum payment amount in stroops. Must be `>= min_amount`.
    ///
    /// # Errors
    /// - [`ContractError::AlreadyInitialized`]: contract has already been initialized.
    /// - [`ContractError::InvalidAmount`]: `min_amount <= 0`.
    /// - [`ContractError::InvalidLimits`]: `min_amount > max_amount`.
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
    ///
    /// Falls back to [`CONTRACT_VERSION`] when the stored version is absent
    /// (e.g. before `init` has been called).
    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Version)
            .unwrap_or(CONTRACT_VERSION)
    }

    /// Update payment amount limits. Admin-only.
    ///
    /// # Parameters
    /// - `min_amount`: Minimum payment amount in stroops. Must be `> 0`.
    /// - `max_amount`: Maximum payment amount in stroops. Must be `>= min_amount`.
    ///
    /// # Errors
    /// - [`ContractError::InvalidAmount`]: `min_amount <= 0`.
    /// - [`ContractError::InvalidLimits`]: `min_amount > max_amount`.
    /// - [`ContractError::ContractNotInitialized`]: contract has not been initialized.
    ///
    /// # Panics
    /// Panics if the stored admin does not authorize the call.
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
    ///
    /// # Parameters
    /// - `new_max`: New maximum number of payments per batch. Must be in the
    ///   range `1..=MAX_BATCH_SIZE` (currently 100).
    ///
    /// # Panics
    /// - If `new_max == 0` or `new_max > MAX_BATCH_SIZE`.
    /// - If the contract has not been initialized.
    /// - If the stored admin does not authorize the call.
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

    /// Set or clear the merchant registry used to gate batch creation. Admin-only.
    ///
    /// # Parameters
    /// - `registry`: `Some(address)` to enable registry gating against the given
    ///   MerchantRegistry contract, or `None` to disable gating.
    ///
    /// # Panics
    /// - If the contract has not been initialized.
    /// - If the stored admin does not authorize the call.
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
    /// # Parameters
    /// - `merchant`: Merchant that owns the payments. Must authorize this call.
    /// - `payments`: Batch of payment inputs. Length must be in the range
    ///   `1..=max_batch_size` (default 100, configurable via
    ///   [`BatchPaymentContract::set_max_batch_size`]). Each `amount` must be
    ///   within `[min_amount, max_amount]` and each `memo` must be non-empty.
    ///
    /// # Returns
    /// A `Vec<BytesN<32>>` of the created payment IDs.
    ///
    /// # Errors
    /// - [`ContractError::ContractNotInitialized`]: contract has not been initialized.
    /// - [`ContractError::BatchEmpty`]: `payments` is empty.
    /// - [`ContractError::InvalidAmount`]: an item's amount is outside the configured limits.
    /// - [`ContractError::EmptyMemo`]: an item's memo is empty.
    ///
    /// # Panics
    /// - If `merchant` does not authorize the call.
    /// - If the merchant exceeds the per-ledger batch limit.
    /// - If a registry is configured and `merchant` is not approved.
    /// - If `payments.len()` exceeds the configured maximum batch size.
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
        

/* … truncated 3448 chars — edit only what you need near the top … */
