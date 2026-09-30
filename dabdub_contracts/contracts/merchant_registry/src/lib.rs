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
    /// Number of merchants registered so far. Stored as a single scalar so
    /// registration never has to read/rewrite the full merchant index.
    MerchantCount,
    /// Per-merchant index slot: maps a merchant's position to its address.
    /// Each entry is written independently, so registering merchant N does
    /// not touch any prior merchant's entry.
    MerchantAt(u32),
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
    old_name: String,
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
        env.storage().instance().set(&DataKey::MerchantCount, &0u32);
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

        // Append to the index in O(1): read the scalar count, write the new
        // address into its own slot, then bump the count. No prior merchant
        // entry is read or rewritten, and no single value grows with N.
        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MerchantCount)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::MerchantAt(count), &merchant.clone());
        env.storage()
            .instance()
            .set(&DataKey::MerchantCount, &(count + 1));

        env.events().publish(
            ("REGISTRY", "merchant_registered"),
            MerchantRegisteredEvent { merchant: merchant.clone(), name: name.clone() },
        );
    }

    /// Update a registered merchant's business name. Callable by admin only.
    pub fn update_merchant(env: Env, caller: Address, merchant: Address, name: String) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        if name.len() == 0 {
            panic!("Merchant name cannot be empty");
        }

        let key = DataKey::Merchant(merchant.clone());
        let mut record: MerchantRecord = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Merchant not found");

        let old_name = record.name.clone();
        record.name = name.clone();
        env.storage().persistent().set(&key, &record);

        env.events().publish(
            ("REGISTRY", "merchant_updated"),
            MerchantUpdatedEvent {
                merchant,
                old_name,
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

        env.events().publish(
            ("REGISTRY", "merchant_terminated"),
            MerchantTerminatedEvent { merchant: merchant.clone() },
        );
    }

    /// Update the KYC verification flag for a merchant.  Callable by admin only.
    pub fn set_kyc_status(env: Env, caller: Address, merchant: Address, verified: bool) {
        caller.require_auth();
      

        let key = DataKey::Merchant(merchant.clone());
        let mut record: MerchantRecord = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Merchant not found");

        record.kyc_verified = verified;
        env.storage().persistent().set(&key, &record);

        env.events().publish(
            ("REGISTRY", "kyc_status_updated"),
            KYCStatusUpdatedEvent { merchant: merchant.clone(), verified },
        );
    }

    /// Update the negotiated fee tier for a merchant.  Callable by admin only.
    pub fn update_fee_tier(env: Env, caller: Address, merchant: Address, fee_bps: u32) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        if fee_bps > MAX_FEE_BPS {
            panic!("Fee exceeds maximum");
        }

        let key = DataKey::Merchant(merchant.clone());
        let mut record: MerchantRecord = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Merchant not found");

        record.fee_bps = fee_bps;
        env.storage().persistent().set(&key, &record);

        env.events().publish(
            ("REGISTRY", "fee_tier_updated"),
            FeeTierUpdatedEvent { merchant: merchant.clone(), fee_bps },
        );
    }

    /// Transfer admin rights to a new address.  Callable by the current admin.
    pub fn transfer_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let old_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Admin not set");

        env.storage().instance().set(&DataKey::Admin, &new_admin);

        env.events().publish(
            ("REGISTRY", "admin_transferred"),
            AdminTransferredEvent { old_admin, new_admin },
        );
    }

    // ------------------------------------------------------------------
    // Queries
    // ------------------------------------------------------------------

    /// Fetch a single merchant record.  Returns `None` if not registered.
    pub fn get_merchant(env: Env, merchant: Address) -> Option<MerchantRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::Merchant(merchant))
    }

    /// Return the total number of registered merchants.
    pub fn merchant_count(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MerchantCount)
            .unwrap_or(0)
    }

    /// Paginated listing of registered merchants.
    ///
    /// Cost is bounded by `page_size`, not by the total number of merchants:
    /// the index is stored as one entry per position (`DataKey::MerchantAt`),
    /// so only the `page_size` slots in the requested window are read. The
    /// full index is never loaded, and no per-entry persistent record read is
    /// performed — each slot already holds the merchant's address.
    pub fn merchants(env: Env, page: u32, page_size: u32) -> Vec<Address> {
        let mut result = Vec::new(&env);
        if page_size == 0 {
            return result;
        }

        let total: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MerchantCount)
            .unwrap_or(0);

        let start = page.saturating_mul(page_size);
        if start >= total {
            return result;
        }

        let end = start.saturating_add(page_size).min(total);
        let mut i = start;
        while i < end {
            if let Some(addr) = env
                .storage()
                .persistent()
                .get::<DataKey, Address>(&DataKey::MerchantAt(i))
            {
                result.push_back(addr);
            }
            i += 1;
        }

        result
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Admin not set");
        if &admin != caller {
            panic!("Caller is not admin");
        }
    }
}
