#![no_std]

mod test;

use soroban_sdk::{
    contract, contractimpl, contracttype, vec, xdr::ToXdr, Address, Bytes, BytesN, Env, String, Vec,
};

const DEFAULT_MAX_BATCH_SIZE: u32 = 20;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    MinAmount,
    MaxAmount,
    MaxBatchSize,
    Counter,
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
    /// Unique payment ID (32-byte hash derived from merchant, a per-contract
    /// counter, and the payment's contents — see issue #1024).
    pub id: BytesN<32>,
    pub amount: i128,
    pub memo: String,
    pub merchant: Address,
}

#[contract]
pub struct BatchPaymentContract;

#[contractimpl]
impl BatchPaymentContract {
    /// One-time initialization for admin and amount limits.
    pub fn init(env: Env, admin: Address, min_amount: i128, max_amount: i128) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("contract already initialized");
        }
        if min_amount <= 0 {
            panic!("min amount must be > 0");
        }
        if min_amount > max_amount {
            panic!("min amount must be <= max amount");
        }

        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::MinAmount, &min_amount);
        env.storage().instance().set(&DataKey::MaxAmount, &max_amount);
        env.storage().instance().set(&DataKey::MaxBatchSize, &DEFAULT_MAX_BATCH_SIZE);
        env.storage().instance().set(&DataKey::Counter, &0u64);
    }

    /// Update payment amount limits. Admin-only.
    pub fn set_limits(env: Env, min_amount: i128, max_amount: i128) {
        if min_amount <= 0 {
            panic!("min amount must be > 0");
        }
        if min_amount > max_amount {
            panic!("min amount must be <= max amount");
        }

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("contract not initialized"));
        admin.require_auth();

        env.storage().instance().set(&DataKey::MinAmount, &min_amount);
        env.storage().instance().set(&DataKey::MaxAmount, &max_amount);
        env.events().publish(
            (soroban_sdk::Symbol::new(&env, "LimitsUpdated"),),
            (min_amount, max_amount),
        );
    }

    /// Update the maximum batch size. Admin-only.
    pub fn set_max_batch_size(env: Env, new_max: u32) {
        if new_max == 0 {
            panic!("max batch size must be > 0");
        }

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("contract not initialized"));
        admin.require_auth();

        env.storage().instance().set(&DataKey::MaxBatchSize, &new_max);
        env.events().publish(
            (soroban_sdk::Symbol::new(&env, "MaxBatchSizeUpdated"),),
            (new_max,),
        );
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
    ) -> Vec<BytesN<32>> {
        merchant.require_auth();

        let min_amount: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MinAmount)
            .unwrap_or_else(|| panic!("contract not initialized"));
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
            panic!("batch must contain at least one payment");
        }
        if count > max_batch_size {
            panic!("batch exceeds maximum of {} payments", max_batch_size);
        }

        // ── Validation pass (all items checked before any state write) ────────
        for i in 0..count {
            let item = payments.get(i).unwrap();
            if item.amount < min_amount || item.amount > max_amount {
                panic!("payment amount outside configured limits");
            }
            if item.memo.len() == 0 {
                panic!("payment at index {}: memo must not be empty", i);
            }
        }

        // ── Creation pass ─────────────────────────────────────────────────────
        let mut payment_ids: Vec<BytesN<32>> = vec![&env];
        let mut counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::Counter)
            .unwrap_or(0u64);

        for i in 0..count {
            let item = payments.get(i).unwrap();

            let new_counter = counter + i as u64;
            let mut seed_bytes = Bytes::new(&env);
            seed_bytes.extend_from_array(&(env.ledger().sequence() as u64).to_be_bytes());
            seed_bytes.extend_from_array(&new_counter.to_be_bytes());
            seed_bytes.append(&merchant.clone().to_xdr(&env));

            let id_bytes: BytesN<32> = env.crypto().sha256(&seed_bytes).into();

            let record = PaymentRecord {
                id: id_bytes.clone(),
                amount: item.amount,
                memo: item.memo.clone(),
                merchant: merchant.clone(),
            };
            env.storage().persistent().set(&DataKey::Payment(id_bytes.clone()), &record);

            let record = PaymentRecord {
                id: id_bytes.clone(),
                amount: item.amount,
                memo: item.memo.clone(),
                merchant: merchant.clone(),
            };
            env.storage()
                .persistent()
                .set(&DataKey::Payment(id_bytes.clone()), &record);

            // Emit PaymentCreated event — one per batch entry.
            env.events().publish(
                (soroban_sdk::Symbol::new(&env, "PaymentCreated"),),
                (id_bytes.clone(), merchant.clone(), item.amount, item.memo.clone()),
            );

            payment_ids.push_back(id_bytes);
        }

        let final_counter = counter + count as u64;
        env.storage().instance().set(&DataKey::Counter, &final_counter);

        payment_ids
    }

    /// Returns the on-chain record for a previously created payment, if any.
    pub
            env.events().publish(
                (soroban_sdk::Symbol::new(&env, "PaymentCreated"),),
                (id_bytes.clone(), merchant.clone(), item.amount, item.memo.clone()),
            );

            payment_ids.push_back(id_bytes);
        }

        let final_counter = counter + count as u64;
        env.storage().instance().set(&DataKey::Counter, &final_counter);

        payment_ids
    }

    /// Returns the on-chain record for a previously created payment, if any.
    pub fn get_payment(env: Env, id: BytesN<32>) -> Option<PaymentRecord> {
        env.storage().persistent().get(&DataKey::Payment(id))
    }

    /// Returns the maximum allowed batch size.
    pub fn max_batch_size(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MaxBatchSize)
            .unwrap_or(DEFAULT_MAX_BATCH_SIZE)
    }

    /// Return the currently configured min and max payment amounts.
    pub fn get_limits(env: Env) -> (i128, i128) {
        let min_amount: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MinAmount)
            .unwrap_or_else(|| panic!("contract not initialized"));
        let max_amount: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MaxAmount)
            .unwrap_or_else(|| panic!("contract not initialized"));
        (min_amount, max_amount)
    }
}
