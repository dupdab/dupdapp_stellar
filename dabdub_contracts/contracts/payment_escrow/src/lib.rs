#![no_std]

mod test;

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, token, Address, BytesN,
    Env, String, Vec,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ContractError {
    InvalidTtl = 1,
    TtlExceedsMaximum = 2,
    InvalidAmount = 3,
    DuplicatePaymentId = 4,
    MerchantNotApproved = 5,
    PaymentNotFound = 6,
    PaymentFullyReleased = 7,
    PaymentExpired = 8,
    PaymentHasNotExpired = 9,
    PaymentNotPending = 10,
    DisputeOpen = 11,
    DisputeNotOpen = 12,
    NotPaymentParticipant = 13,
    DisputeWindowExpired = 14,
    InvalidDisputeWinner = 15,
    NotAdmin = 16,
    ReleaseFundsExceedRemaining = 17,
    NotEmergencySigner = 18,
    EmergencyDrainCooldownActive = 19,
    NoFundsToDrain = 20,
    InvalidEmergencyCooldown = 21,
    InvalidSignerCount = 22,
    DuplicateEmergencySigners = 23,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum AssetType {
    Xlm,
    Usdc,
}

/// Thin client interface for the MerchantRegistry contract.
#[contractclient(name = "MerchantRegistryClient")]
#[allow(dead_code)]
trait MerchantRegistry {
    fn is_approved(env: Env, merchant: Address) -> bool;
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum PaymentStatus {
    Pending,
    Disputed,
    Released,
    Expired,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct PaymentEscrow {
    pub payment_id: BytesN<32>,
    pub amount: i128,
    pub released_amount: i128,
    pub merchant: Address,
    pub customer: Address,
    pub status: PaymentStatus,
    pub expiry: u32,
    pub dispute_window_end: u32,
    pub dispute_resolution_deadline: u32,
    pub dispute_reason: Option<String>,
    pub asset_type: AssetType,
}

// Storage strategy (see docs/storage-strategy.md for full rationale):
//
// * Instance storage holds contract-wide configuration (admin, token addresses,
//   registry, emergency signers, upgrade timelock, version, limits). It is
//   loaded on every invocation and shares the contract's TTL, so it is kept
//   alive by `extend_ttl` on each deposit.
// * Persistent storage holds per-payment `PaymentEscrow` records keyed by
//   `DataKey::Payment(payment_id)`. Each record is independently archivable and
//   must outlive its business expiry, so `deposit` extends its TTL to
//   `ttl_ledgers + 1` to keep the record readable at the refund point.
// * Temporary storage is not used: escrowed funds and their records must never
//   be silently evicted, so no data here is safe to lose on expiry.

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    XlmToken,
    UsdcToken,
    DefaultTtlLedgers,
    RegistryContract,
    EmergencySigners,
    EmergencyTreasury,
    EmergencyCooldownLedgers,
    EmergencyLastDrainLedger,
    UpgradeDelayLedgers,
    PendingUpgradeHash,
    PendingUpgradeLedger,
    Payment(BytesN<32>),
    Version,
    MaxDisputeWindowLedgers,
    MaxTtlLedgers,
}

#[contracttype]
struct DepositEvent {
    payment_id: BytesN<32>,
    customer: Address,
    merchant: Address,
    amount: i128,
    expiry: u32,
}

#[contracttype]
struct EscrowStateEvent {
    payment_id: BytesN<32>,
    amount: i128,
}

#[contracttype]
struct EmergencyDrainEvent {
    amount: i128,
    caller: Address,
}

#[contracttype]
struct RegistryUpdatedEvent {
    previous: Option<Address>,
    new: Option<Address>,
    caller: Address,
}

#[contracttype]
struct LimitUpdatedEvent {
    previous: u32,
    new: u32,
}

#[contracttype]
struct UpgradeScheduledEvent {
    new_wasm_hash: BytesN<32>,
    apply_ledger: u32,
}

#[contracttype]
struct UpgradeCancelledEvent {
    new_wasm_hash: BytesN<32>,
}

/// Defaults used until the admin overrides them via `set_max_dispute_window_ledgers`
/// / `set_max_ttl_ledgers`.
const DEFAULT_MAX_DISPUTE_WINDOW_LEDGERS: u32 = 51_840;
// 518,400 ledgers is approximately 30 days at 5 seconds per ledger. This
// default bounds how long a new payment may tie up capital while allowing a
// settlement window; it is not a wall-clock or regulatory deadline. The admin
// can change the cap for future deposits via set_max_ttl_ledgers. Business
// expiry and Soroban archival are separate: deposit extends storage through
// expiry + 1 so the first refundable ledger remains accessible.
const DEFAULT_MAX_TTL_LEDGERS: u32 = 518_400;
const DEFAULT_DISPUTE_RESOLUTION_DELAY_LEDGERS: u32 = 17_280;
const EMERGENCY_SIGNER_COUNT: u32 = 3;
const DEFAULT_UPGRADE_DELAY_LEDGERS: u32 = 17_280;

#[contract]
pub struct PaymentEscrowContract;

#[contractimpl]
impl PaymentEscrowContract {
    pub fn __constructor(
        env: Env,
        admin: Address,
        xlm_token: Address,
        usdc_token: Address,
        default_ttl_ledgers: u32,
        registry: Option<Address>,
        emergency_signers: Vec<Address>,
        emergency_treasury: Address,
        emergency_cooldown_ledgers: u32,
    ) -> Result<(), ContractError> {
        if default_ttl_ledgers == 0 {
            return Err(ContractError::InvalidTtl);
        }
        if emergency_cooldown_ledgers == 0 {
            return Err(ContractError::InvalidEmergencyCooldown);
        }
        if emergency_signers.len() != EMERGENCY_SIGNER_COUNT {
            return Err(ContractError::InvalidSignerCount);
        }
        let signer_0 = emergency_signers.get(0).unwrap();
        let signer_1 = emergency_signers.get(1).unwrap();
        let signer_2 = emergency_signers.get(2).unwrap();
        if signer_0 == signer_1 || signer_0 == signer_2 || signer_1 == signer_2 {
            return Err(ContractError::DuplicateEmergencySigners);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::XlmToken, &xlm_token);
        env.storage()
            .instance()
            .set(&DataKey::UsdcToken, &usdc_token);
        env.storage()
            .instance()
            .set(&DataKey::DefaultTtlLedgers, &default_ttl_ledgers);
        if let Some(reg) = registry {
            env.storage()
                .instance()
                .set(&DataKey::RegistryContract, &reg);
        }
        env.storage()
            .instance()
            .set(&DataKey::EmergencySigners, &emergency_signers);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyTreasury, &emergency_treasury);
        env.storage().instance().set(
            &DataKey::EmergencyCooldownLedgers,
            &emergency_cooldown_ledgers,
        );
        env.storage()
            .instance()
            .set(&DataKey::EmergencyLastDrainLedger, &0u32);

        // Initialize the upgrade timelock delay to a safe default.
        env.storage().instance().set(
            &DataKey::UpgradeDelayLedgers,
            &DEFAULT_UPGRADE_DELAY_LEDGERS,
        );

        // Initialize contract version to 1.
        env.storage().instance().set(&DataKey::Version, &1u32);
        Ok(())
    }

    /// Schedule a WASM upgrade. Admin-only. The upgrade does not take effect
    /// immediately: it must wait out the configured timelock delay and then be
    /// applied via `apply_upgrade`. This gives affected parties time to notice
    /// and react before new code can control escrowed funds.
    pub fn schedule_upgrade(env: Env, caller: Address, new_wasm_hash: BytesN<32>) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let delay: u32 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeDelayLedgers)
            .unwrap_or(DEFAULT_UPGRADE_DELAY_LEDGERS);
        let apply_ledger = env.ledger().sequence().saturating_add(delay);

        env.storage()
            .instance()
            .set(&DataKey::PendingUpgradeHash, &new_wasm_hash);
        env.storage()
            .instance()
            .set(&DataKey::PendingUpgradeLedger, &apply_ledger);

        env.events().publish(
            ("ESCROW", "upgrade_scheduled"),
            UpgradeScheduledEvent {
                new_wasm_hash,
                apply_ledger,
            },
        );
    }

    /// Cancel a previously scheduled upgrade. Admin-only.
    pub fn cancel_upgrade(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let pending: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::PendingUpgradeHash)
            .unwrap_or_else(|| panic!("No pending upgrade"));

        env.storage()
            .instance()
            .remove(&DataKey::PendingUpgradeHash);
        env.storage()
            .instance()
            .remove(&DataKey::PendingUpgradeLedger);

        env.events().publish(
            ("ESCROW", "upgrade_cancelled"),
            UpgradeCancelledEvent {
                new_wasm_hash: pending,
            },
        );
    }

    /// Apply a previously scheduled upgrade once the timelock delay has
    /// elapsed. Admin-only. Reverts if no upgrade is pending or the delay has
    /// not yet passed.
    pub fn apply_upgrade(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        let new_wasm_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::PendingUpgradeHash)
            .unwrap_or_else(|| panic!("No pending upgrade"));
        let apply_ledger: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PendingUpgradeLedger)
            .unwrap_or_else(|| panic!("No pending upgrade"));

        if env.ledger().sequence() < apply_ledger {
            panic!("Upgrade timelock has not elapsed");
        }

        env.storage()
            .instance()
            .remove(&DataKey::PendingUpgradeHash);
        env.storage()
            .instance()
            .remove(&DataKey::PendingUpgradeLedger);

        let version: u32 = env.storage().instance().get(&DataKey::Version).unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::Version, &(version + 1));

        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());
        env.events().publish(
            ("ESCROW", "upgrade_applied"),
            (new_wasm_hash, version + 1, apply_ledger),
        );
    }

    /// Return the current contract version.
    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Version).unwrap_or(0)
    }

    /// Return the currently pending upgrade hash and its apply ledger, if any.
    pub fn get_pending_upgrade(env: Env) -> Option<(BytesN<32>, u32)> {
        let hash: Option<BytesN<32>> = env.storage().instance().get(&DataKey::PendingUpgradeHash);
        let apply_ledger: Option<u32> =
            env.storage().instance().get(&DataKey::PendingUpgradeLedger);
        match (hash, apply_ledger) {
            (Some(h), Some(l)) => Some((h, l)),
            _ => None,
        }
    }

    /// Update (or remove) the merchant registry address.  Admin-only.
    ///
    /// Removing the registry (`None`) disables merchant approval/KYC gating for
    /// ALL future deposits, so it additionally requires co-authorization from two
    /// distinct emergency signers. Every call emits a `REGISTRY` event whose topic
    /// is `set`, `changed` or `removed`, with the previous and new addresses.
    pub fn set_registry(
        env: Env,
        caller: Address,
        registry: Option<Address>,
        removal_signers: Option<(Address, Address)>,
    ) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let previous: Option<Address> = env.storage().instance().get(&DataKey::RegistryContract);
        let action = match (&previous, &registry) {
            (_, None) => {
                let (s1, s2) =
                    removal_signers.expect("Registry removal requires emergency signers");
                if s1 == s2 {
                    panic!("Emergency signers must be distinct");
                }
                s1.require_auth();
                s2.require_auth();
                Self::require_emergency_signer(&env, &s1);
                Self::require_emergency_signer(&env, &s2);
                env.storage().instance().remove(&DataKey::RegistryContract);
                "removed"
            }
            (prev, Some(reg)) => {
                env.storage()
                    .instance()
                    .set(&DataKey::RegistryContract, reg);
                if prev.is_some() {
                    "changed"
                } else {
                    "set"
                }
            }
        };

        env.events().publish(
            ("REGISTRY", action),
            RegistryUpdatedEvent {
                previous,
                new: registry,
                caller,
            },
        );
    }

    /// Update the maximum dispute window (in ledgers). Admin-only; must be > 0.
    pub fn set_max_dispute_window_ledgers(env: Env, caller: Address, value: u32) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        if value == 0 {
            panic!("Max dispute window must be > 0");
        }
        let previous = Self::get_max_dispute_window_ledgers(env.clone());
        env.storage()
            .instance()
            .set(&DataKey::MaxDisputeWindowLedgers, &value);
        env.events().publish(
            ("CONFIG", "max_dispute_window"),
            LimitUpdatedEvent {
                previous,
                new: value,
            },
        );
    }

    /// Update the maximum payment TTL (in ledgers). Admin-only; must be > 0.
    /// Only affects future deposits.
    pub fn set_max_ttl_ledgers(env: Env, caller: Address, value: u32) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        if value == 0 {
            panic!("Max TTL must be > 0");
        }
        let previous = Self::get_max_ttl_ledgers(env.clone());
        env.storage()
            .instance()
            .set(&DataKey::MaxTtlLedgers, &value);
        env.events().publish(
            ("CONFIG", "max_ttl"),
            LimitUpdatedEvent {
                previous,
                new: value,
            },
        );
    }

    /// Return the registry contract address if one is configured.
    pub fn get_registry(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::RegistryContract)
    }

    pub fn deposit(
        env: Env,
        customer: Address,
        payment_id: BytesN<32>,
        merchant: Address,
        amount: i128,
        ttl_ledgers: u32,
        asset_type: AssetType,
    ) -> Result<BytesN<32>, ContractError> {
        customer.require_auth();

        if amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }
        if ttl_ledgers == 0 {
            return Err(ContractError::InvalidTtl);
        }
        if ttl_ledgers > Self::get_max_ttl_ledgers(env.clone()) {
            panic!("TTL exceeds maximum");
        }

        // Storage TTL is relative to the current ledger and live-until is
        // inclusive. Refunds require sequence > expiry, hence one extra ledger.
        let retention = ttl_ledgers
            .checked_add(1)
            .filter(|ttl| *ttl <= env.storage().max_ttl())
            .expect("TTL exceeds storage retention limit");
        let expiry = env
            .ledger()
            .sequence()
            .checked_add(ttl_ledgers)
            .expect("Payment expiry overflow");

        let key = DataKey::Payment(payment_id.clone());
        if env.storage().persistent().has(&key) {
            return Err(ContractError::DuplicatePaymentId);
        }

        // If a registry contract is configured, verify the merchant is approved.
        if let Some(registry_addr) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::RegistryContract)
        {
            let registry_client = MerchantRegistryClient::new(&env, &registry_addr);
            if !registry_client.is_approved(&merchant) {
                return Err(ContractError::MerchantNotApproved);
            }
        }

        // Transfer funds into the contract via the appropriate token interface.
        let token_addr = Self::token_address(&env, &asset_type);
        token::Client::new(&env, &token_addr).transfer(
            &customer,
            &env.current_contract_address(),
            &amount,
        );

        let dispute_window_end = {
            let max_window_end = env
                .ledger()
                .sequence()
                .saturating_add(Self::get_max_dispute_window_ledgers(env.clone()));
            if expiry < max_window_end {
                expiry
            } else {
                max_window_end
            }
        };

        let payment = PaymentEscrow {
            payment_id: payment_id.clone(),
            amount,
            released_amount: 0,
            merchant: merchant.clone(),
            customer: customer.clone(),
            status: PaymentStatus::Pending,
            expiry,
            dispute_window_end,
            dispute_resolution_deadline: 0,
            dispute_reason: None,
            asset_type,
        };

        env.storage().persistent().set(&key, &payment);
        env.storage()
            .persistent()
            .extend_ttl(&key, retention, retention);
        // Keep the escrow code and configuration callable for the same period.
        // Extending for a shorter new payment never shortens existing TTLs.
        env.storage().instance().extend_ttl(retention, retention);

        env.events().publish(
            ("ESCROW", "deposit"),
            DepositEvent {
                payment_id: payment_id.clone(),
                customer,
                merchant,
                amount,
                expiry,
            },
        );

        Ok(payment_id)
    }

    pub fn release(env: Env, caller: Address, payment_id: BytesN<32>) -> Result<(), ContractError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        let mut payment = Self::get_payment(env.clone(), payment_id.clone())?;
        Self::require_releasable(&env, &payment)?;

        let remaining = Self::remaining_amount(&payment);

        Self::transfer_from_contract(&env, &payment.merchant, remaining, &payment.asset_type);
        payment.released_amount = payment.amount;
        payment.status = PaymentStatus::Released;
        let release_key = DataKey::Payment(payment_id.clone());
        env.storage().persistent().set(&release_key, &payment);
        env.storage()
            .persistent()
            .extend_ttl(&release_key, MAX_TTL_LEDGERS, MAX_TTL_LEDGERS);

        env.events().publish(
            ("ESCROW", "release"),
            EscrowStateEvent {
                payment_id,
                amount: remaining,
            },
        );
        Ok(())
    }

    pub fn release_partial(env: Env, caller: Address, payment_id: BytesN<32>, amount: i128) -> Result<(), ContractError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        if amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }

        let mut payment = Self::get_payment(env.clone(), payment_id.clone())?;
        Self::require_releasable(&env, &payment)?;

        let remaining = Self::remaining_amount(&payment);
        if remaining <= 0 {
            return Err(ContractError::PaymentFullyReleased);
        }
        if amount > remaining {
            return Err(ContractError::ReleaseFundsExceedRemaining);
        }

        Self::transfer_from_contract(&env, &payment.merchant, amount, &payment.asset_type);
        payment.released_amount += amount;
        if payment.released_amount == payment.amount {
            payment.status = PaymentStatus::Released;
        }
        let partial_key = DataKey::Payment(payment_id.clone());
        env.storage().persistent().set(&partial_key, &payment);
        env.storage()
            .persistent()
            .extend_ttl(&partial_key, MAX_TTL_LEDGERS, MAX_TTL_LEDGERS);

        env.events().publish(
            ("ESCROW", "release"),
            EscrowStateEvent { payment_id, amount },
        );
        Ok(())
    }

    pub fn expire(env: Env, payment_id: BytesN<32>) -> Result<(), ContractError> {
        let mut payment = Self::get_payment(env.clone(), payment_id.clone())?;
        Self::require_expirable(&env, &payment)?;

        let remaining = Self::remaining_amount(&payment);
        if remaining <= 0 {
            return Err(ContractError::PaymentFullyReleased);
        }
        Self::transfer_from_contract(&env, &payment.customer, remaining, &payment.asset_type);

        payment.released_amount = payment.amount;
        payment.status = PaymentStatus::Expired;
        let expire_key = DataKey::Payment(payment_id.clone());
        env.storage().persistent().set(&expire_key, &payment);
        env.storage()
            .persistent()
            .extend_ttl(&expire_key, MAX_TTL_LEDGERS, MAX_TTL_LEDGERS);

        env.events().publish(
            ("ESCROW", "expired"),
            EscrowStateEvent {
                payment_id,
                amount: remaining,
            },
        );
        Ok(())
    }

    pub fn refund(env: Env, payment_id: BytesN<32>) -> Result<(), ContractError> {
        let mut payment = Self::get_payment(env.clone(), payment_id.clone())?;
        Self::require_expirable(&env, &payment)?;

        let remaining = Self::remaining_amount(&payment);
        if remaining <= 0 {
            return Err(ContractError::PaymentFullyReleased);
        }

        Self::transfer_from_contract(&env, &payment.customer, remaining, &payment.asset_type);
        payment.released_amount = payment.amount;
        payment.status = PaymentStatus::Expired;
        let refund_key = DataKey::Payment(payment_id.clone());
        env.storage().persistent().set(&refund_key, &payment);
        env.storage()
            .persistent()
            .extend_ttl(&refund_key, MAX_TTL_LEDGERS, MAX_TTL_LEDGERS);

        env.events().publish(
            ("ESCROW", "refund"),
            EscrowStateEvent {
                payment_id,
                amount: remaining,
            },
        );
        Ok(())
    }

    /// Opens a dispute on a payment still in `Pending` status.
    ///
    /// Note (#1023): `release_partial` keeps a payment `Pending` until it is
    /// fully released, so a payment that already had one or more partial
    /// releases sent to the merchant can still be disputed here. This is
    /// intentional: `resolve_dispute` only ever moves `remaining_amount()`
    /// (`amount - released_amount`) to the winner, so a customer disputing
    /// after accepting partial delivery recovers only the undelivered
    /// remainder — funds already released to the merchant are not clawed
    /// back. See `test_dispute_after_partial_release_resolves_to_customer`
    /// and `test_dispute_after_partial_release_resolves_to_merchant`.
    pub fn dispute(env: Env, caller: Address, payment_id: BytesN<32>, _reason: String) -> Result<(), ContractError> {
        caller.require_auth();

        let mut payment = Self::get_payment(env.clone(), payment_id.clone())?;
        if payment.status == PaymentStatus::Disputed {
            return Err(ContractError::DisputeOpen);
        }
        if payment.status != PaymentStatus::Pending {
            return Err(ContractError::PaymentNotPending);
        }
        if caller != payment.customer && caller != payment.merchant {
            return Err(ContractError::NotPaymentParticipant);
        }
        if env.ledger().sequence() > payment.dispute_window_end {
            return Err(ContractError::DisputeWindowExpired);
        }

        let resolution_deadline = env
            .ledger()
            .sequence()
            .checked_add(DEFAULT_DISPUTE_RESOLUTION_DELAY_LEDGERS)
            .expect("Dispute resolution deadline overflow");
        payment.status = PaymentStatus::Disputed;
        payment.dispute_resolution_deadline = resolution_deadline;
        payment.dispute_reason = Some(_reason.clone());
        let dispute_key = DataKey::Payment(payment_id.clone());
        env.storage().persistent().set(&dispute_key, &payment);
        env.storage()
            .persistent()
            .extend_ttl(&dispute_key, MAX_TTL_LEDGERS, MAX_TTL_LEDGERS);

        env.events().publish(
            ("ESCROW", "dispute"),
            EscrowStateEvent {
                payment_id,
                amount: Self::remaining_amount(&payment),
            },
        );
        Ok(())
    }

    /// Refunds the remaining disputed balance to the customer after the admin
    /// resolution window has elapsed. Anyone may trigger this fallback.
    pub fn auto_resolve_dispute(env: Env, payment_id: BytesN<32>) {
        let mut payment = Self::get_payment(env.clone(), payment_id.clone());
        if payment.status != PaymentStatus::Disputed {
            panic!("Dispute is not open");
        }
        if env.ledger().sequence() <= payment.dispute_resolution_deadline {
            panic!("Dispute resolution deadline has not passed");
        }

        let remaining = Self::remaining_amount(&payment);
        if remaining <= 0 {
            panic!("Payment fully released");
        }

        Self::transfer_from_contract(&env, &payment.customer, remaining, &payment.asset_type);
        payment.status = PaymentStatus::Expired;
        payment.released_amount = payment.amount;
        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id.clone()), &payment);

        env.events().publish(
            ("ESCROW", "dispute_auto_resolved"),
            EscrowStateEvent {
                payment_id,
                amount: remaining,
            },
        );
    }

    pub fn resolve_dispute(env: Env, caller: Address, payment_id: BytesN<32>, winner: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        let mut payment = Self::get_payment(env.clone(), payment_id.clone());
        // Intentionally NO expiry check (unlike `require_releasable` /
        // `require_expirable`): a dispute can only be opened before
        // `dispute_window_end` (which is <= `expiry`), and once open the funds are
        // frozen until the admin rules on it. Enforcing expiry here would leave a
        // disputed payment that outlives its expiry with no settlement path, since
        // `expire`/`refund` reject non-Pending payments. The dispute resolution
        // deadline instead bounds this admin path and enables the customer refund
        // fallback.
        if payment.status != PaymentStatus::Disputed {
            return Err(ContractError::DisputeNotOpen);
        }
        if env.ledger().sequence() > payment.dispute_resolution_deadline {
            panic!("Dispute resolution deadline has passed");
        }
        if winner != payment.customer && winner != payment.merchant {
            return Err(ContractError::InvalidDisputeWinner);
        }

        let remaining = Self::remaining_amount(&payment);
        if remaining <= 0 {
            return Err(ContractError::PaymentFullyReleased);
        }

        Self::transfer_from_contract(&env, &winner, remaining, &payment.asset_type);
        payment.status = if winner == payment.merchant {
            PaymentStatus::Released
        } else {
            PaymentStatus::Expired
        };
        payment.released_amount = payment.amount;
        let resolve_key = DataKey::Payment(payment_id.clone());
        env.storage().persistent().set(&resolve_key, &payment);
        env.storage()
            .persistent()
            .extend_ttl(&resolve_key, MAX_TTL_LEDGERS, MAX_TTL_LEDGERS);

        let topic = if winner == payment.merchant {
            "release"
        } else {
            "refund"
        };
        env.events().publish(
            ("ESCROW", topic),
            EscrowStateEvent {
                payment_id,
                amount: remaining,
            },
        );
        Ok(())
    }

    /// Resolves an open dispute by splitting the remaining balance between the
    /// merchant and the customer. Admin-only. `merchant_amount` is released to
    /// the merchant and `customer_amount` is refunded to the customer; the two
    /// must sum to the remaining balance. Emits a separate `release` event for
    /// the merchant portion and a `refund` event for the customer portion.
    pub fn resolve_dispute_split(
        env: Env,
        caller: Address,
        payment_id: BytesN<32>,
        merchant_amount: i128,
        customer_amount: i128,
    ) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let mut payment = Self::get_payment(env.clone(), payment_id.clone());
        if payment.status != PaymentStatus::Disputed {
            panic!("Dispute is not open");
        }
        if env.ledger().sequence() > payment.dispute_resolution_deadline {
            panic!("Dispute resolution deadline has passed");
        }

        let remaining = Self::remaining_amount(&payment);
        if remaining <= 0 {
            panic!("Payment fully released");
        }
        if merchant_amount < 0 || customer_amount < 0 {
            panic!("Split amounts must be >= 0");
        }
        if merchant_amount.saturating_add(customer_amount) != remaining {
            panic!("Split amounts must equal remaining balance");
        }

        if merchant_amount > 0 {
            Self::transfer_from_contract(
                &env,
                &payment.merchant,
                merchant_amount,
                &payment.asset_type,
            );
            env.events().publish(
                ("ESCROW", "release"),
                EscrowStateEvent {
                    payment_id: payment_id.clone(),
                    amount: merchant_amount,
                },
            );
        }
        if customer_amount > 0 {
            Self::transfer_from_contract(
                &env,
                &payment.customer,
                customer_amount,
                &payment.asset_type,
            );
            env.events().publish(
                ("ESCROW", "refund"),
                EscrowStateEvent {
                    payment_id: payment_id.clone(),
                    amount: customer_amount,
                },
            );
        }

        payment.released_amount = payment.amount;
        payment.status = PaymentStatus::Released;
        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id.clone()), &payment);
    }

    pub fn get_payment(env: Env, payment_id: BytesN<32>) -> PaymentEscrow {
        let key = DataKey::Payment(payment_id);
        let payment: PaymentEscrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Payment not found");
        // Extend TTL on every read to prevent archival before payment expiry.
        env.storage()
            .persistent()
            .extend_ttl(&key, MAX_TTL_LEDGERS, MAX_TTL_LEDGERS);
        payment
    }

    pub fn get_balance(env: Env, payment_id: BytesN<32>) -> Result<i128, ContractError> {
        let payment = Self::get_payment(env, payment_id)?;
        Ok(Self::remaining_amount(&payment))
    }

    pub fn get_expiry(env: Env, payment_id: BytesN<32>) -> Result<u32, ContractError> {
        Ok(Self::get_payment(env, payment_id)?.expiry)
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    pub fn get_xlm_token(env: Env) -> Address {
        env.storage().instance().get(&DataKey::XlmToken).unwrap()
    }

    pub fn get_usdc_token(env: Env) -> Address {
        env.storage().instance().get(&DataKey::UsdcToken).unwrap()
    }

    pub fn get_default_ttl_ledgers(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::DefaultTtlLedgers)
            .unwrap()
    }

    pub fn get_max_dispute_window_ledgers(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MaxDisputeWindowLedgers)
            .unwrap_or(DEFAULT_MAX_DISPUTE_WINDOW_LEDGERS)
    }

    pub fn get_max_ttl_ledgers(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MaxTtlLedgers)
            .unwrap_or(DEFAULT_MAX_TTL_LEDGERS)
    }

    pub fn emergency_drain(env: Env, caller: Address, signer_one: Address, signer_two: Address) -> Result<i128, ContractError> {
        caller.require_auth();
        if signer_one == signer_two {
            return Err(ContractError::DuplicateEmergencySigners);
        }
        signer_one.require_auth();
        signer_two.require_auth();
        Self::require_emergency_signer(&env, &signer_one)?;
        Self::require_emergency_signer(&env, &signer_two)?;

        let current_ledger = env.ledger().sequence();
        let last_drain_ledger: u32 = env
            .storage()
            .instance()
            .get(&DataKey::EmergencyLastDrainLedger)
            .unwrap_or(0);
        let cooldown_ledgers: u32 = env
            .storage()
            .instance()
            .get(&DataKey::EmergencyCooldownLedgers)
            .unwrap();
        if last_drain_ledger != 0
            && current_ledger < last_drain_ledger.saturating_add(cooldown_ledgers)
        {
            return Err(ContractError::EmergencyDrainCooldownActive);
        }

        let usdc_token: Address = env.storage().instance().get(&DataKey::UsdcToken).unwrap();
        let xlm_token: Address = env.storage().instance().get(&DataKey::XlmToken).unwrap();
        let treasury: Address = env
            .storage()
            .instance()
            .get(&DataKey::EmergencyTreasury)
            .unwrap();
        let contract_address = env.current_contract_address();
        let usdc_client = token::Client::new(&env, &usdc_token);
        let xlm_client = token::Client::new(&env, &xlm_token);
        let usdc_amount = usdc_client.balance(&contract_address);
        let xlm_amount = xlm_client.balance(&contract_address);
        if usdc_amount == 0 && xlm_amount == 0 {
            return Err(ContractError::NoFundsToDrain);
        }

        if usdc_amount > 0 {
            usdc_client.transfer(&contract_address, &treasury.clone(), &usdc_amount);
        }
        if xlm_amount > 0 {
            xlm_client.transfer(&contract_address, &treasury, &xlm_amount);
        }
        let amount = usdc_amount
            .checked_add(xlm_amount)
            .expect("drained amount overflow");
        env.storage()
            .instance()
            .set(&DataKey::EmergencyLastDrainLedger, &current_ledger);
        env.events().publish(
            ("ESCROW", "emergency_drain"),
            EmergencyDrainEvent { amount, caller },
        );

        Ok(amount)
    }

    fn require_admin(env: &Env, caller: &Address) -> Result<(), ContractError> {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if caller != &admin {
            return Err(ContractError::NotAdmin);
        }
        Ok(())
    }

    fn require_releasable(env: &Env, payment: &PaymentEscrow) -> Result<(), ContractError> {
        if payment.status == PaymentStatus::Disputed {
            return Err(ContractError::DisputeOpen);
        }
        if payment.status == PaymentStatus::Released {
            return Err(ContractError::PaymentFullyReleased);
        }
        if payment.status == PaymentStatus::Expired {
            return Err(ContractError::PaymentExpired);
        }
        if env.ledger().sequence() > payment.expiry {
            return Err(ContractError::PaymentExpired);
        }
        Ok(())
    }

    fn require_expirable(env: &Env, payment: &PaymentEscrow) -> Result<(), ContractError> {
        if payment.status == PaymentStatus::Disputed {
            return Err(ContractError::DisputeOpen);
        }
        if payment.status == PaymentStatus::Released {
            return Err(ContractError::PaymentFullyReleased);
        }
        if payment.status != PaymentStatus::Pending {
            return Err(ContractError::PaymentNotPending);
        }
        if env.ledger().sequence() <= payment.expiry {
            return Err(ContractError::PaymentHasNotExpired);
        }
        Ok(())
    }

    fn remaining_amount(payment: &PaymentEscrow) -> i128 {
        payment.amount.saturating_sub(payment.released_amount)
    }

    fn token_address(env: &Env, asset_type: &AssetType) -> Address {
        match asset_type {
            AssetType::Xlm => env.storage().instance().get(&DataKey::XlmToken).unwrap(),
            AssetType::Usdc => env.storage().instance().get(&DataKey::UsdcToken).unwrap(),
        }
    }

    /// Single outbound transfer path used by `release`, `release_partial`, `expire`,
    /// `refund` and `resolve_dispute`. There is deliberately no per-payment fallback:
    /// if the configured token contract becomes non-functional (paused by its issuer,
    /// SAC reverting, etc.) every settlement call for payments in that asset fails
    /// atomically and the payment record is left unchanged. See the "Stuck payment
    /// recovery" section of `EMERGENCY_RUNBOOK.md` for the operator procedure.
    fn transfer_from_contract(
        env: &Env,
        recipient: &Address,
        amount: i128,
        asset_type: &AssetType,
    ) {
        let token_addr = Self::token_address(env, asset_type);
        token::Client::new(env, &token_addr).transfer(
            &env.current_contract_address(),
            recipient,
            &amount,
        );
    }

    fn require_emergency_signer(env: &Env, signer: &Address) -> Result<(), ContractError> {
        let signers: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::EmergencySigners)
            .unwrap();
        if signers.iter().all(|configured| configured != *signer) {
            return Err(ContractError::NotEmergencySigner);
        }
        Ok(())
    }
}
