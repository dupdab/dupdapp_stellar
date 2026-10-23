#![no_std]

use soroban_sdk::{contract, contractimpl, contracterror, contracttype, Address, BytesN, Env};

// TTL constants, in ledgers (~5 seconds per ledger).
const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_BUMP_THRESHOLD: u32 = 7 * DAY_IN_LEDGERS;
const INSTANCE_BUMP_TARGET: u32 = 30 * DAY_IN_LEDGERS;
const PERSISTENT_BUMP_THRESHOLD: u32 = 30 * DAY_IN_LEDGERS;
const PERSISTENT_BUMP_TARGET: u32 = 365 * DAY_IN_LEDGERS;

/// Window, in ledgers, during which a nonce is considered live (replay protection).
const NONCE_WINDOW_LEDGERS: u32 = 2 * DAY_IN_LEDGERS;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Instance: privileged admin address.
    Admin,
    /// Instance: fee and configuration parameters.
    Config,
    /// Instance: monotonic settlement counter.
    SettlementCount,
    /// Persistent: settlement record by id.
    Settlement(u64),
    /// Persistent: external payment id -> settlement id.
    PaymentIndex(BytesN<32>),
    /// Temporary: replay guard for (payer, nonce).
    Nonce(Address, u64),
}

#[contracttype]
#[derive(Clone)]
pub struct Config {
    pub fee_bps: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct Settlement {
    pub id: u64,
    pub payment_id: BytesN<32>,
    pub payer: Address,
    pub payee: Address,
    pub amount: i128,
    pub fee: i128,
    pub ledger: u32,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    InvalidAmount = 4,
    NonceUsed = 5,
    PaymentExists = 6,
    SettlementNotFound = 7,
    InvalidFee = 8,
}

#[contract]
pub struct SettlementLedger;

fn extend_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_TARGET);
}

fn extend_persistent(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_TARGET);
}

fn read_admin(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)
}

fn require_admin(env: &Env) -> Result<(), Error> {
    let admin = read_admin(env)?;
    admin.require_auth();
    Ok(())
}

#[contractimpl]
impl SettlementLedger {
    /// Initialise the contract. Can only be called once.
    pub fn initialize(env: Env, admin: Address, fee_bps: u32) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        if fee_bps > 10_000 {
            return Err(Error::InvalidFee);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Config, &Config { fee_bps });
        env.storage().instance().set(&DataKey::SettlementCount, &0u64);
        extend_instance(&env);
        Ok(())
    }

    /// Update fee configuration. Admin only.
    pub fn set_fee(env: Env, fee_bps: u32) -> Result<(), Error> {
        require_admin(&env)?;
        if fee_bps > 10_000 {
            return Err(Error::InvalidFee);
        }
        env.storage()
            .instance()
            .set(&DataKey::Config, &Config { fee_bps });
        extend_instance(&env);
        Ok(())
    }

    /// Record a settlement for a payment. Enforces replay protection via a
    /// temporary nonce and uniqueness of the payment id via a persistent index.
    pub fn settle(
        env: Env,
        payment_id: BytesN<32>,
        payer: Address,
        payee: Address,
        amount: i128,
        nonce: u64,
    ) -> Result<u64, Error> {
        payer.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let nonce_key = DataKey::Nonce(payer.clone(), nonce);
        if env.storage().temporary().has(&nonce_key) {
            return Err(Error::NonceUsed);
        }
        env.storage().temporary().set(&nonce_key, &true);
        env.storage()
            .temporary()
            .extend_ttl(&nonce_key, NONCE_WINDOW_LEDGERS, NONCE_WINDOW_LEDGERS);

        let index_key = DataKey::PaymentIndex(payment_id.clone());
        if env.storage().persistent().has(&index_key) {
            return Err(Error::PaymentExists);
        }

        let config: Config = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(Error::NotInitialized)?;
        let fee = amount * (config.fee_bps as i128) / 10_000;

        let mut count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::SettlementCount)
            .unwrap_or(0);
        count += 1;
        env.storage()
            .instance()
            .set(&DataKey::SettlementCount, &count);

        let settlement = Settlement {
            id: count,
            payment_id: payment_id.clone(),
            payer,
            payee,
            amount,
            fee,
            ledger: env.ledger().sequence(),
        };

        let settlement_key = DataKey::Settlement(count);
        env.storage().persistent().set(&settlement_key, &settlement);
        extend_persistent(&env, &settlement_key);

        env.storage().persistent().set(&index_key, &count);
        extend_persistent(&env, &index_key);

        extend_instance(&env);
        Ok(count)
    }

    /// Fetch a settlement by id. Bumps its TTL so frequently read records stay live.
    pub fn get_settlement(env: Env, id: u64) -> Result<Settlement, Error> {
        let key = DataKey::Settlement(id);
        let settlement: Settlement = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::SettlementNotFound)?;
        extend_persistent(&env, &key);
        Ok(settlement)
    }

    /// Look up a settlement id by external payment id.
    pub fn get_settlement_id(env: Env, payment_id: BytesN<32>) -> Option<u64> {
        let key = DataKey::PaymentIndex(payment_id);
        let id: Option<u64> = env.storage().persistent().get(&key);
        if id.is_some() {
            extend_persistent(&env, &key);
        }
        id
    }

    /// Maintenance: extend the TTL of the instance and a set of settlement ids.
    /// Intended to be called periodically by a keeper so idle records do not lapse.
    pub fn extend_settlements(env: Env, ids: soroban_sdk::Vec<u64>) {
        extend_instance(&env);
        for id in ids.iter() {
            extend_persistent(&env, &DataKey::Settlement(id));
        }
    }
}
