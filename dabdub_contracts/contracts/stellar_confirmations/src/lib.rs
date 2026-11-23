#![no_std]

mod test;

use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, Address, BytesN, Env, String,
};

// ── payment_escrow cross-contract interface ─────────────────────────────────
//
// Minimal mirror of payment_escrow's record and get_payment entry point.
// Field names, types, and order must stay in sync with payment_escrow.

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

// ── errors ──────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Error {
    InvalidConfirmationThreshold = 1,
}

// ── storage keys ────────────────────────────────────────────────────────────

#[contracttype]
enum DataKey {
    Admin,
    ConfirmationCount,               // required ledger-close count (u32)
    ExpiryLedgers,                   // ledgers after first confirmation before expiry (u32)
    PaymentConfs(BytesN<32>),        // confirmed count so far for a payment (u32)
    PaymentFirstLedger(BytesN<32>),  // ledger_seq of the first confirmation
    PaymentLastLedger(BytesN<32>),   // ledger_seq of the most recent confirmation
    PaymentRequiredConfs(BytesN<32>),// required count snapshotted at first confirmation
    PaymentSettling(BytesN<32>),     // bool — already transitioned
    PaymentFailed(BytesN<32>),       // bool — terminal failure state
    PaymentExpired(BytesN<32>),      // bool — terminal expiry state
    PaymentEscrowContract,           // optional Address of the payment_escrow contract
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
    ledger_seq: u32,
    amount: i128,
    merchant: Address,
}

#[contracttype]
struct ConfirmationFailedEvent {
    payment_id: BytesN<32>,
    confirmations: u32,
    required: u32,
}

#[contracttype]
struct ConfirmationExpiredEvent {
    payment_id: BytesN<32>,
    confirmations: u32,
    required: u32,
    first_ledger: u32,
    expired_at_ledger: u32,
}

// ── contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct StellarConfirmationsContract;

#[contractimpl]
impl StellarConfirmationsContract {
    pub fn __constructor(env: Env, admin: Address, confirmation_count: u32) -> Result<(), Error> {
        Self::validate_confirmation_threshold(confirmation_count)?;
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::ConfirmationCount, &confirmation_count);
        Ok(())
    }

    /// Validates that a confirmation threshold is at least 1.
    /// Returns `Error::InvalidConfirmationThreshold` for 0.
    fn validate_confirmation_threshold(confirmation_count: u32) -> Result<(), Error> {
        if confirmation_count == 0 {
            return Err(Error::InvalidConfirmationThreshold);
        }
        Ok(())
    }

    /// Called by the NestJS monitor once per observed Stellar ledger close.
    /// `ledger_seq` is the sequence number of the closed ledger.
    /// `amount` and `merchant` are forwarded into the SettlementAuthorised event
    /// so NestJS can trigger the fiat payout without a second lookup. When a
    /// payment_escrow contract has been configured via `set_payment_escrow_contract`,
    /// the final threshold-crossing call cross-checks `amount`/`merchant` against
    /// `payment_escrow::get_payment` and panics on a mismatch.
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

        // Guard: ledger_seq must not be in the future relative to the actual chain state
        assert!(
            ledger_seq <= env.ledger().sequence(),
            "ledger_seq exceeds current ledger"
        );

        // Guard: ledger_seq must not regress relative to the last one recorded for this payment
        let last_key = DataKey::PaymentLastLedger(payment_id.clone());
        if let Some(last_seen) = env.storage().persistent().get::<_, u32>(&last_key) {
            assert!(
                ledger_seq >= last_seen,
                "ledger_seq precedes last recorded ledger"
            );
        }

        // Guard: already settling — idempotent no-op after threshold
        let settling_key = DataKey::PaymentSettling(payment_id.clone());
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&settling_key)
            .unwrap_or(false)
        {
            panic!("payment already settling");
        }

        // Guard: already failed — terminal state, cannot resume confirming
        let failed_key = DataKey::PaymentFailed(payment_id.clone());
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&failed_key)
            .unwrap_or(false)
        {
            panic!("payment already failed");
        }

        // Guard: already expired — terminal state, cannot resume confirming
        let expired_key = DataKey::PaymentExpired(payment_id.clone());
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&expired_key)
            .unwrap_or(false)
        {
            panic!("payment already expired");
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

        // Record last-seen ledger for the monotonicity check above
        env.storage().persistent().set(&last_key, &ledger_seq);

        // Snapshot the required confirmation count at first confirmation so
        // later admin changes to the global ConfirmationCount don't retroactively
        // move the threshold for payments already accumulating confirmations.
        let required_key = DataKey::PaymentRequiredConfs(payment_id.clone());
        let required: u32 = match env.storage().persistent().get(&required_key) {
            Some(snapshotted) => snapshotted,
            None => {
                let current: u32 = env
                    .storage()
                    .instance()
                    .get(&DataKey::ConfirmationCount)
                    .unwrap();
                env.storage().persistent().set(&required_key, &current);
                current
            }
        };

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
            // If a payment_escrow contract is configured, cross-check the
            // caller-supplied amount/merchant against the real escrow record.
            if let Some(escrow_address) = Self::get_payment_escrow_contract(env.clone()) {
                let escrow_client = PaymentEscrowClient::new(&env, &escrow_address);
                let record = escrow_client.get_payment(&payment_id);
                assert!(record.amount == amount, "amount mismatch with escrow");
                assert!(record.merchant == merchant, "merchant mismatch with escrow");
            }

            env.storage().persistent().set(&settling_key, &true);

            env.events().publish(
                ("STELLAR_CONFS", "settlement_authorised"),
                SettlementAuthorisedEvent {
                    payment_id: payment_id.clone(),
                    ledger_seq,
                    amount,
                    merchant,
                },
            );
        }

        new_confs
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        assert!(caller == &admin, "caller is not admin");
    }

    fn get_payment_escrow_contract(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get(&DataKey::PaymentEscrowContract)
    }
}
