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
            panic!("Cannot reactivate terminated merchant");
        }

        record.status = MerchantStatus::Active;
        env.storage().persistent().set(&key, &record);

        env.events().publish(
            ("REGISTRY", "merchant_reactivated"),
            MerchantReactivatedEvent { merchant: merchant.clone() },
        );
    }

    /// Permanently terminate a merchant.  Callable by admin only.
    /// Unlike suspension, termination is irreversible: a terminated
    /// merchant can never be reactivated (see `reactivate_merchant`) or
    /// suspended again (see `suspend_merchant`).
    ///
    /// The terminated merchant's address is also removed from the
    /// `DataKey::Merchants` index so that `merchants()` no longer lists it.
    pub fn terminate_merchant(env: Env, caller: Address, merchant: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let key = DataKey::Merchant(merchant.clone());
        let mut record: MerchantRecord = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Merchant not found");

        if record.status == MerchantStatus::Terminated {
            panic!("Merchant already terminated");
        }

        record.status = MerchantStatus::Terminated;
        env.storage().persistent().set(&key, &record);

        // Remove from the listing index so `merchants()` no longer lists it.
        let merchants: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Merchants)
            .unwrap();
        let mut updated = Vec::<Address>::new(&env);
        for addr in merchants.iter() {
            if addr != merchant {
                updated.push_back(addr);
            }
        }
        env.storage().instance().set(&DataKey::Merchants, &updated);

        env.events().publish(
            ("REGISTRY", "merchant_terminated"),
            MerchantTerminatedEvent { merchant: merchant.clone() },
        );
    }

    // ------------------------------------------------------------------
    // Status queries
    // ------------------------------------------------------------------

    /// Rich status query that distinguishes an unregistered merchant from a
    /// registered merchant that is suspended, terminated, or not KYC
    /// verified.  The boolean conveniences below remain available and keep
    /// their existing semantics.
    pub fn get_merchant_status(env: Env, merchant: Address) -> MerchantStatusInfo {
        let key = DataKey::Merchant(merchant.clone());
        match env.storage().persistent().get::<DataKey, MerchantRecord>(&key) {
            None => MerchantStatusInfo {
                registered: false,
                status: None,
                is_active: false,
                is_suspended: false,
                is_terminated: false,
                is_kyc_verified: false,
                is_approved: false,
            },
            Some(record) => {
                let is_active = record.status == MerchantStatus::Active;
                let is_suspended = record.status == MerchantStatus::Suspended;
                let is_terminated = record.status == MerchantStatus::Terminated;
                MerchantStatusInfo {
                    registered: true,
                    status: Some(record.status.clone()),
                    is_active,
                    is_suspended,
                    is_terminated,
                    is_kyc_verified: record.kyc_verified,
                    is_approved: is_active && record.kyc_verified,
                }
            }
        }
    }

    /// Returns `true` only when the merchant is registered, active, and KYC
    /// verified.  Returns `false` for unregistered, suspended, terminated,
    /// or not-KYC-verified merchants; use `get_merchant_status` to tell those
    /// cases apart.
    pub fn is_approved(env: Env, merchant: Address) -> bool {
        let key = DataKey::Merchant(merchant.clone());
        match env.storage().persistent().get::<DataKey, MerchantRecord>(&key) {
            None => false,
            Some(record) => {
                record.status == MerchantStatus::Active && record.kyc_verified
            }
        }
    }

    /// Returns `true` only when the merchant is registered and active.
    pub fn is_merchant_active(env: Env, merchant: Address) -> bool {
        let key = DataKey::Merchant(merchant.clone());
        match env.storage().persistent().get::<DataKey, MerchantRecord>(&key) {
            None => false,
            Some(record) => record.status == MerchantStatus::Active,
        }
    }

    /// Returns `true` only when the merchant is registered and KYC verified.
    pub fn is_kyc_verified(env: Env, merchant: Address) -> bool {
        let key = DataKey::Merchant(merchant.clone());
        match env.storage().persistent().get::<DataKey, MerchantRecord>(&key) {
            None => false,
            Some(record) => record.kyc_verified,
        }
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if &admin != caller {
            panic!("Caller is not admin");
        }
    }
}
