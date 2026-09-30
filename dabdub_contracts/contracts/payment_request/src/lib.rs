#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaymentStatus {
    Pending = 0,
    Confirmed = 1,
    Settling = 2,
    Settled = 3,
    Failed = 4,
    Expired = 5,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payment {
    pub id: String,
    pub merchant: Address,
    pub amount: i128,
    pub asset: Address,
    pub status: PaymentStatus,
    pub created_at: u64,
    pub expiry: u64,
}

/// TTL policy for payment records (in ledgers, ~5s each).
/// Active payments are bumped on every write so they never lapse mid-flow.
/// Terminal payments (Settled/Failed/Expired) get a shorter TTL and are left to
/// expire naturally, acting as the archival strategy: off-chain indexers should
/// persist the emitted events as the long-term record.
const ACTIVE_TTL_THRESHOLD: u32 = 17_280; // ~1 day
const ACTIVE_TTL_EXTEND: u32 = 518_400; // ~30 days
const TERMINAL_TTL_THRESHOLD: u32 = 17_280; // ~1 day
const TERMINAL_TTL_EXTEND: u32 = 120_960; // ~7 days
const MAX_ID_LEN: u32 = 64;

#[contracttype]
pub enum DataKey {
    Payment(String),
}

#[contract]
pub struct PaymentRequestContract;

fn is_terminal(status: &PaymentStatus) -> bool {
    matches!(
        status,
        PaymentStatus::Settled | PaymentStatus::Failed | PaymentStatus::Expired
    )
}

/// Persist a payment and set its TTL according to its lifecycle stage.
fn save_payment(env: &Env, key: &DataKey, payment: &Payment) {
    let storage = env.storage().persistent();
    storage.set(key, payment);
    if is_terminal(&payment.status) {
        storage.extend_ttl(key, TERMINAL_TTL_THRESHOLD, TERMINAL_TTL_EXTEND);
    } else {
        storage.extend_ttl(key, ACTIVE_TTL_THRESHOLD, ACTIVE_TTL_EXTEND);
    }
}

/// Load a payment and require authorization from its merchant.
fn load_authorized(env: &Env, key: &DataKey) -> Payment {
    let payment: Payment = env.storage().persistent().get(key).expect("Payment not found");
    payment.merchant.require_auth();
    payment
}

#[contractimpl]
impl PaymentRequestContract {
    /// Initialize a new payment request.
    pub fn create_payment(
        env: Env,
        id: String,
        merchant: Address,
        amount: i128,
        asset: Address,
        expiry: u64,
    ) {
        merchant.require_auth();
        if id.len() == 0 {
            panic!("payment id must not be empty");
        }
        if id.len() > MAX_ID_LEN {
            panic!("payment id too long");
        }
        let key = DataKey::Payment(id.clone());
        if env.storage().persistent().has(&key) {
            panic!("Payment already exists");
        }

        let payment = Payment {
            id: id.clone(),
            merchant,
            amount,
            asset,
            status: PaymentStatus::Pending,
            created_at: env.ledger().timestamp(),
            expiry,
        };

        save_payment(&env, &key, &payment);

        env.events().publish(
            (symbol_short!("payment"), id, symbol_short!("created")),
            PaymentStatus::Pending,
        );
    }

    /// Mark payment as confirmed (user has paid).
    pub fn confirm(env: Env, id: String) {
        let key = DataKey::Payment(id.clone());
        let mut payment = load_authorized(&env, &key);

        if payment.status != PaymentStatus::Pending {
            panic!("Invalid transition: can only confirm pending payments");
        }

        if env.ledger().timestamp() > payment.expiry {
            panic!("Payment expired");
        }

        payment.status = PaymentStatus::Confirmed;
        save_payment(&env, &key, &payment);

        env.events().publish(
            (symbol_short!("payment"), id, symbol_short!("confirmed")),
            PaymentStatus::Confirmed,
        );
    }

    /// Move payment to settling state (initiate payout to merchant).
    pub fn set_settling(env: Env, id: String) {
        let key = DataKey::Payment(id.clone());
        let mut payment = load_authorized(&env, &key);

        if payment.status != PaymentStatus::Confirmed {
            panic!("Invalid transition: can only set settling from confirmed");
        }

        payment.status = PaymentStatus::Settling;
        save_payment(&env, &key, &payment);

        env.events().publish(
            (symbol_short!("payment"), id, symbol_short!("settling")),
            PaymentStatus::Settling,
        );
    }

    /// Mark payment as settled (funds received by merchant).
    pub fn settle(env: Env, id: String) {
        let key = DataKey::Payment(id.clone());
        let mut payment = load_authorized(&env, &key);

        // Settling is a mandatory intermediate step: `set_settling` signals that a
        // payout is in flight, so settle may only follow it.
        if payment.status != PaymentStatus::Settling {
            panic!("Invalid transition: can only settle from Settling");
        }

        payment.status = PaymentStatus::Settled;
        save_payment(&env, &key, &payment);

        env.events().publish(
            (symbol_short!("payment"), id, symbol_short!("settled")),
            PaymentStatus::Settled,
        );
    }

    /// Mark payment as failed.
    pub fn fail(env: Env, id: String) {
        let key = DataKey::Payment(id.clone());
        let mut payment = load_authorized(&env, &key);

        if payment.status == PaymentStatus::Settled || payment.status == PaymentStatus::Expired {
            panic!("Cannot fail a finalized payment");
        }

        payment.status = PaymentStatus::Failed;
        save_payment(&env, &key, &payment);

        env.events().publish(
            (symbol_short!("payment"), id, symbol_short!("failed")),
            PaymentStatus::Failed,
        );
    }

    /// Mark payment as expired.
    pub fn expire(env: Env, id: String) {
        let key = DataKey::Payment(id.clone());
        let mut payment = load_authorized(&env, &key);

        if payment.status != PaymentStatus::Pending && payment.status != PaymentStatus::Confirmed {
            panic!("Cannot expire a finalized or settling payment");
        }

        if env.ledger().timestamp() <= payment.expiry {
            panic!("Payment has not yet reached expiry time");
        }

        payment.status = PaymentStatus::Expired;
        save_payment(&env, &key, &payment);

        env.events().publish(
            (symbol_short!("payment"), id, symbol_short!("expired")),
            PaymentStatus::Expired,
        );
    }

    /// Fetch payment details.
    pub fn get_payment(env: Env, id: String) -> Payment {
        env.storage().persistent().get(&DataKey::Payment(id)).expect("Payment not found")
    }
}

mod test;
