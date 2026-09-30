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
    pub fee_bps: u32,
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

        // Remove the terminated merchant from the listing index so that
        // `merchants()` no longer returns it.
        let merchants: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Merchants)
            .unwrap();
        let mut remaining: Vec<Address> = Vec::new(&env);
        for existing in merchants.iter() {
            if existing != merchant {
                remaining.push_back(existing);
            }
        }
        env.storage().instance().set(&DataKey::Merchants, &remaining);

        env.events().publish(
            ("REGISTRY", "merchant_terminated"),
            MerchantTerminatedEvent { merchant: merchant.clone() },
        );
    }

    /// Set the KYC verification status for a merchant.  Admin-only.
    pub fn set_kyc_status(env: Env, caller: Address, merchant: Address, verified: bool) {
        caller.require_auth();
      

/* … truncated 5419 chars — edit only what you need near the top … */
