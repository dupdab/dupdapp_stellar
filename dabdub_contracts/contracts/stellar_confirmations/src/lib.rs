#![no_std]

mod test;

use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, Address, BytesN, Env, String,
};

// ── payment_escrow cross-contract interface ─────────────────────────────────
//
// Minimal mirror of payment_escrow's `PaymentEscrow` record and `get_payment`
// entry point, following the same local-trait pattern liquidity_router uses
// for its AmmClient (see contracts/liquidity_router/src/lib.rs). Field names,
// types, and order must stay in sync with payment_escrow::PaymentEscrow.

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum PaymentEscrowAssetType {
    Xlm,
    Usdc,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum PaymentEscrowStatus {
    Pending,
    Disputed,
    Released,
    Expired,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct PaymentEscrowRecord {
    pub payment_id: BytesN<32>,
    pub amount: i128,
    pub released_amount: i128,
    pub merchant: Address,
    pub customer: Address,
    pub status: PaymentEscrowStatus,
    pub expiry: u32,
    pub dispute_window_end: u32,
    pub dispute_reason: Option<String>,
    pub asset_type: PaymentEscrowAssetType,
}

#[contractclient(name = "PaymentEscrowClient")]
pub trait PaymentEscrowInterface {
    fn get_payment(env: Env, payment_id: BytesN<32>) -> PaymentEscrowRecord;
}

// ── storage keys ────────────────────────────────────────────────────────────

#[contracttype]
enum DataKey {
    Admin,
    ConfirmationCount,          // required ledger-close count (u32)
    PaymentConfs(BytesN<32>),   // confirmed count so far for a payment (u32)
    PaymentFirstLedger(BytesN<32>), // ledger_seq of the first confirmation
    PaymentSettling(BytesN<32>),    // bool — already transitioned
    PaymentEscrowContract,      // Address of the payment_escrow contract (optional)
}

// ── events ───────────────────────────────────────────────────────────────────

#[contracttype]
struct ConfirmationRecordedEvent {
    payment_id: BytesN<32>,
    ledger_seq: u32,
    confirmations: u32,
    required: u32,
}

#[contracttype]
struct SettlementAuthorisedEvent {
    payment_id: BytesN<32>,
    ledger_seq: u32,   // ledger that pushed count over threshold
    amount: i128,      // informational — set by caller
    merchant: Address, // informational — set by caller
}

// ── contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct StellarConfirmationsContract;

#[contractimpl]
impl StellarConfirmationsContract {
    pub fn __constructor(env: Env, admin: Address, confirmation_count: u32) {
        assert!(confirmation_count > 0, "confirmation_count must be > 0");
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::ConfirmationCount, &confirmation_count);
    }

    /// Called by the NestJS monitor once per observed Stellar ledger close.
    /// `ledger_seq` is the sequence number of the closed ledger.
    /// `amount` and `merchant` are forwarded into the SettlementAuthorised event
    /// so NestJS can trigger the fiat payout without a second lookup.
    ///
    /// Returns the current confirmation count after this call.
    pub fn confirm_payment(
        env: Env,
        caller: Address,
        payment_id: BytesN<32>,
        ledger_seq: u32,
        amount: i128,
        merchant: Address,
    ) -> u32 {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Cross-contract check: if a payment_escrow contract is linked, verify
        // `amount`/`merchant` against the real on-chain payment record for
        // `payment_id` before recording this confirmation. This closes the gap
        // where a compromised/malfunctioning monitor could fabricate
        // confirmation/settlement events for arbitrary amount/merchant values.
        if let Some(escrow_address) = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::PaymentEscrowContract)
        {
            let escrow_client = PaymentEscrowClient::new(&env, &escrow_address);
            let payment = escrow_client.get_payment(&payment_id);
            assert!(payment.amount == amount, "amount mismatch with payment_escrow record");
            assert!(payment.merchant == merchant, "merchant mismatch with payment_escrow record");
        }

        // Guard: already settling — idempotent no-op after threshold
        let settling_key = DataKey::PaymentSettling(payment_id.clone());
        if env.storage().persistent().get::<_, bool>(&settling_key).unwrap_or(false) {
            panic!("payment already settling");
        }

        // Increment confirmation counter
        let confs_key = DataKey::PaymentConfs(payment_id.clone());
        let confs: u32 = env.storage().persistent().get(&confs_key).unwrap_or(0);
        let new_confs = confs.checked_add(1).expect("overflow");
        env.storage().persistent().set(&confs_key, &new_confs);

        // Record first-seen ledger for auditability
        let first_key = DataKey::PaymentFirstLedger(payment_id.clone());
        if !env.storage().persistent().has(&first_key) {
            env.storage().persistent().set(&first_key, &ledger_seq);
        }

        let required: u32 = env.storage().instance().get(&DataKey::ConfirmationCount).unwrap();

        env.events().publish(
            ("STELLAR_CONFS", "confirmation_recorded"),
            ConfirmationRecordedEvent {
                payment_id: payment_id.clone(),
                ledger_seq,
                confirmations: new_confs,
                required,
            },
        );

        // Transition to Settling when threshold is reached
        if new_confs >= required {
            env.storage().persistent().set(&settling_key, &true);

            env.events().publish(
                ("STELLAR_CONFS", "settlement_authorised"),
                SettlementAuthorisedEvent {
                    payment_id,
                    ledger_seq,
                    amount,
                    merchant,
                },
            );
        }

        new_confs
    }

    /// Admin: update the required confirmation count.
    pub fn set_confirmation_count(env: Env, caller: Address, count: u32) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        assert!(count > 0, "confirmation_count must be > 0");
        env.storage().instance().set(&DataKey::ConfirmationCount, &count);
    }

    pub fn get_confirmation_count(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::ConfirmationCount).unwrap()
    }

    /// Admin: link the payment_escrow contract used to cross-check
    /// `amount`/`merchant` in `confirm_payment`. Pass `None` to unlink.
    pub fn set_payment_escrow_contract(env: Env, caller: Address, contract: Option<Address>) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        match contract {
            Some(addr) => env
                .storage()
                .instance()
                .set(&DataKey::PaymentEscrowContract, &addr),
            None => env.storage().instance().remove(&DataKey::PaymentEscrowContract),
        }
    }

    pub fn get_payment_escrow_contract(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::PaymentEscrowContract)
    }

    pub fn get_payment_confirmations(env: Env, payment_id: BytesN<32>) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::PaymentConfs(payment_id))
            .unwrap_or(0)
    }

    pub fn is_settling(env: Env, payment_id: BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::PaymentSettling(payment_id))
            .unwrap_or(false)
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        assert!(caller == &admin, "not admin");
    }
}
