#![no_std]

mod test;

use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env, String, Vec,
};

/// Lifecycle states for a registered merchant.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum MerchantStatus {
    Active,
    Suspended,
    Terminated,
}

/// On-chain merchant record stored in Persistent storage.
#[contracttype]
#[derive(Clone, Debug)]
pub struct MerchantRecord {
    pub merchant: Address,
    pub name: String,
    pub status: MerchantStatus,
    pub kyc_verified: bool,
    /// Negotiated fee rate for this merchant, in basis points (1/100th of a
    /// percent). Defaults to `DEFAULT_FEE_BPS` at registration and can be
    /// overridden per-merchant by the admin via `update_fee_tier`.
    ///
    /// `0` is a valid, intentional fee rate (0%) and is distinct from an
    /// "unset" value: every registered merchant always has an explicit
    /// `fee_bps` (seeded with `DEFAULT_FEE_BPS` at registration), so a `0`
    /// read back from `get_fee_tier`/`get_merchant` means the admin
    /// deliberately set this merchant's rate to 0% via `update_fee_tier`
    /// (e.g. a VIP or promotional arrangement), not that no fee was ever
    /// configured.
    pub fee_bps: u32,
}

/// Rich merchant status query result.
///
/// Unlike the boolean conveniences (`is_approved`, `is_merchant_active`,
/// `is_kyc_verified`), which collapse "unregistered" and "registered but not
/// meeting the condition" into the same `false`, this struct lets callers
/// distinguish the exact reason a merchant is not usable.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct MerchantStatusInfo {
    /// `true` if a merchant record exists for the queried address.
    pub registered: bool,
    /// Lifecycle state; `None` when the merchant is not registered.
    pub status: Option<MerchantStatus>,
    /// `true` only when registered and `status == Active`.
    pub is_active: bool,
    /// `true` only when registered and `status == Suspended`.
    pub is_suspended: bool,
    /// `true` only when registered and `status == Terminated`.
    pub is_terminated: bool,
    /// `true` only when registered and KYC verified.
    pub is_kyc_verified: bool,
    /// `true` only when registered, active, and KYC verified.
    pub is_approved: bool,
}

/// Default fee rate applied to newly registered merchants: 150 bps (1.5%).
const DEFAULT_FEE_BPS: u32 = 150;
/// Upper bound on the fee rate an admin can set for a merchant: 1000 bps (10%).
const MAX_FEE_BPS: u32 = 1000;

/// Storage keys used by the registry.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    /// Pending admin proposed via `propose_admin`, awaiting `accept_admin`.
    PendingAdmin,
    Merchant(Address),
    /// Index of all registered merchant addresses, for paginated listing.
    Merchants,
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[contracttype]
struct MerchantRegisteredEvent {
    merchant: Address,
    name: String,
}

#[contracttype]
struct MerchantSuspendedEvent {
    merchant: Address,
}

#[contracttype]
struct MerchantReactivatedEvent {
    merchant: Address,
}

#[contracttype]
struct KYCStatusUpdatedEvent {
    merchant: Address,
    verified: bool,
}

#[contracttype]
struct AdminTransferProposedEvent {
    current_admin: Address,
    pending_admin: Address,
}

#[contracttype]
struct AdminTransferredEvent {
    old_admin: Address,
    new_admin: Address,
}

#[contracttype]
struct FeeTierUpdatedEvent {
    merchant: Address,
    fee_bps: u32,
}

#[contracttype]
struct MerchantTerminatedEvent {
    merchant: Address,
}

#[contracttype]
struct MerchantUpdatedEvent {
    merchant: Address,
    name: String,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct MerchantRegistryContract;

#[contractimpl]
impl MerchantRegistryContract {
    // ------------------------------------------------------------------
    // Constructor
    // ------------------------------------------------------------------

    pub fn __constructor(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Merchants, &Vec::<Address>::new(&env));
    }

    // ------------------------------------------------------------------
    // Admin – two-step handover
    // ------------------------------------------------------------------

    /// Propose a new admin. Callable by the current admin only.
    ///
    /// The proposal does not take effect until the proposed address calls
    /// `accept_admin`, so a typo'd or unreachable `new_admin` cannot lock the
    /// registry: the current admin stays in control and can re-propose (or
    /// correct) the pending address at any time.
    pub fn propose_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &new_admin);

        env.events().publish(
            ("REGISTRY", "admin_transfer_proposed"),
            AdminTransferProposedEvent {
                current_admin: caller,
                pending_admin: new_admin,
            },
        );
    }

    /// Complete a pending admin transfer. Callable by the pending admin only.
    ///
    /// Requires the pending admin's own `require_auth`, so the transfer only
    /// takes effect once the proposed address proves control of itself.
    pub fn accept_admin(env: Env) {
        let pending: Address = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .expect("No pending admin transfer");

        pending.require_auth();

        let old_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Admin not set");

        env.storage().instance().set(&DataKey::Admin, &pending);
        env.storage().instance().remove(&DataKey::PendingAdmin);

        env.events().publish(
            ("REGISTRY", "admin_transferred"),
            AdminTransferredEvent {
                old_admin,
                new_admin: pending,
            },
        );
    }

    /// Read the pending admin, if a transfer has been proposed.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::PendingAdmin)
    }

    // ------------------------------------------------------------------
    // Admin – merchant lifecycle
    // ------------------------------------------------------------------

    /// Register a new merchant.  Callable by admin only.
    pub fn register_merchant(env: Env, caller: Address, merchant: Address, name: String) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let key = DataKey::Merchant(merchant.clone());
        if env.storage().persistent().has(&key) {
            panic!("Merchant already registered");
        }

        let record = MerchantRecord {
            merchant: merchant.clone(),
            name: name.clone(),
            status: MerchantStatus::Active,
            kyc_verified: false,
            fee_bps: DEFAULT_FEE_BPS,
        };
        env.storage().persistent().set(&key, &record);

        let mut merchants: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Merchants)
            .unwrap();
        merchants.push_back(merchant.clone());
        env.storage().instance().set(&DataKey::Merchants, &merchants);

        env.events().publish(
            ("REGISTRY", "merchant_registered"),
            MerchantRegisteredEvent { merchant: merchant.clone(), name: name.clone() },
        );
    }

    /// Update a registered merchant's business name. Callable by admin only.
    pub fn update_merchant(env: Env, caller: Address, merchant: Address, name: String) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let key = DataKey::Merchant(merchant.clone());
        let mut record: MerchantRecord = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Merchant not found");

        record.name = name.clone();
        env.storage().persistent().set(&key, &record);

        env.events().publish(
            ("REGISTRY", "merchant_updated"),
            MerchantUpdatedEvent {
                merchant,
                name,
            },
        );
    }

    /// Suspend a merchant.  Callable by admin only.
    /// After suspension, the Escrow contract will reject new deposits for
    /// this merchant.
    pub fn suspend_merchant(env: Env, caller: Address, merchant: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let key = DataKey::Merchant(merchant.clone());
        let mut record: MerchantRecord = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Merchant not found");

        if record.status == MerchantStatus::Suspended {
            panic!("Merchant already suspended");
        }
        if record.status == MerchantStatus::Terminated {
            panic!("Merchant is terminated");
        }

        record.status = MerchantStatus::Suspended;
        env.storage().persistent().set(&key, &record);

        env.events().publish(
            ("REGISTRY", "merchant_suspended"),
            MerchantSuspendedEvent { merchant: merchant.clone() },
        );
    }

    /// Reactivate a previously suspended merchant.  Callable by admin only.
    pub fn reactivate_merchant(env: Env, caller: Address, merchant: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let key = DataKey::Merchant(merchant.clone());
        let mut record: MerchantRecord = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Merchant not found");

        if record.status == MerchantStatus::Active {
            panic!("Merchant already active");
        }
        if record.status == MerchantStatus::Terminated {
            panic!("Cannot reac

/* … truncated 5375 chars — edit only what you need near the top … */
