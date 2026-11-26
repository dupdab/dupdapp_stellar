#![no_std]

mod test;

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, String, Vec};

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

/// Maximum combined size, in bytes, of a merchant's metadata fields. Larger
/// payloads would create oversized ledger entries, so registration rejects them.
const MAX_METADATA_BYTES: u32 = 1024;

/// Errors surfaced by the registry.
#[contracterror]
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// Merchant metadata exceeded {@link MAX_METADATA_BYTES} (1KB).
    MetadataTooLarge = 1,
}

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
    /// The merchant's public key (the address that controls the merchant).
    public_key: Address,
    /// Ledger close time, in seconds, when registration happened.
    timestamp: u64,
}

#[contracttype]
struct MerchantSuspendedEvent {
    merchant: Address,
    /// Why the merchant was suspended (e.g. `"suspended_by_admin"`).
    reason: String,
    /// Ledger close time, in seconds, when the suspension happened.
    suspended_at: u64,
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

        // Reject oversized metadata before writing anything, so a single
        // registration can never create an unbounded ledger entry.
        if Self::metadata_len(&name) > MAX_METADATA_BYTES {
            panic!("{:?}", Error::MetadataTooLarge);
        }

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

        let mut merchants: Vec<Address> =
            env.storage().instance().get(&DataKey::Merchants).unwrap();
        merchants.push_back(merchant.clone());
        env.storage()
            .instance()
            .set(&DataKey::Merchants, &merchants);

        env.events().publish(
            ("REGISTRY", "merchant_registered"),
            MerchantRegisteredEvent {
                merchant: merchant.clone(),
                name: name.clone(),
                public_key: merchant.clone(),
                timestamp: env.ledger().timestamp(),
            },
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
            MerchantUpdatedEvent { merchant, name },
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
            MerchantSuspendedEvent {
                merchant: merchant.clone(),
                reason: String::from_str(&env, "suspended_by_admin"),
                suspended_at: env.ledger().timestamp(),
            },
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
            MerchantReactivatedEvent {
                merchant: merchant.clone(),
            },
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
            MerchantTerminatedEvent {
                merchant: merchant.clone(),
            },
        );
    }

    /// Set the KYC verification status for a merchant.  Admin-only.
    pub fn set_kyc_status(env: Env, caller: Address, merchant: Address, verified: bool) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

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
            KYCStatusUpdatedEvent {
                merchant: merchant.clone(),
                verified,
            },
        );
    }

    /// Set a merchant's negotiated fee rate, in basis points. Admin-only.
    /// Panics if `fee_bps` exceeds `MAX_FEE_BPS` (1000 bps / 10%).
    pub fn update_fee_tier(env: Env, caller: Address, merchant: Address, fee_bps: u32) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        if fee_bps > MAX_FEE_BPS {
            panic!("fee_bps exceeds maximum of 1000");
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
            FeeTierUpdatedEvent {
                merchant: merchant.clone(),
                fee_bps,
            },
        );
    }

    // ------------------------------------------------------------------
    // Queries
    // ------------------------------------------------------------------

    pub fn get_merchant(env: Env, merchant: Address) -> MerchantRecord {
        env.storage()
            .persistent()
            .get(&DataKey::Merchant(merchant))
            .expect("Merchant not found")
    }

    /// Paginated list of all registered merchants.
    pub fn merchants(env: Env, page: u32, page_size: u32) -> Vec<MerchantRecord> {
        if page_size == 0 {
            panic!("page size must be > 0");
        }
        let all: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Merchants)
            .unwrap_or(Vec::new(&env));

        let start = (page as u64)
            .saturating_mul(page_size as u64)
            .min(all.len() as u64) as u32;
        let end = (start as u64 + page_size as u64).min(all.len() as u64) as u32;

        let mut out = Vec::new(&env);
        let iter = all.slice(start..end);
        for addr in iter.iter() {
            out.push_back(Self::get_merchant(env.clone(), addr));
        }
        out
    }

    /// Returns `true` when the merchant is registered and Active.
    pub fn is_merchant_active(env: Env, merchant: Address) -> bool {
        let key = DataKey::Merchant(merchant);
        if !env.storage().persistent().has(&key) {
            return false;
        }
        let record: MerchantRecord = env.storage().persistent().get(&key).unwrap();
        record.status == MerchantStatus::Active
    }

    /// Returns `true` when the merchant is registered, Active, and KYC verified.
    /// Used by callers (e.g. payment_escrow) to gate deposits on merchant approval.
    /// Returns `false` for unregistered merchants.
    pub fn is_approved(env: Env, merchant: Address) -> bool {
        let key = DataKey::Merchant(merchant);
        if !env.storage().persistent().has(&key) {
            return false;
        }
        let record: MerchantRecord = env.storage().persistent().get(&key).unwrap();
        record.status == MerchantStatus::Active && record.kyc_verified
    }

    /// Returns `true` when the merchant is KYC verified.
    /// Returns `false` for unregistered merchants.
    pub fn is_kyc_verified(env: Env, merchant: Address) -> bool {
        let key = DataKey::Merchant(merchant);
        if !env.storage().persistent().has(&key) {
            return false;
        }
        let record: MerchantRecord = env.storage().persistent().get(&key).unwrap();
        record.kyc_verified
    }

    /// Returns the merchant's current fee rate in basis points.
    /// Panics if the merchant is not registered.
    pub fn get_fee_tier(env: Env, merchant: Address) -> u32 {
        Self::get_merchant(env, merchant).fee_bps
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    // ------------------------------------------------------------------
    // Admin management
    // ------------------------------------------------------------------

    /// Transfer admin to a new address.  Callable by current admin only.
    pub fn transfer_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let old_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        env.storage().instance().set(&DataKey::Admin, &new_admin);

        env.events().publish(
            ("REGISTRY", "admin_transferred"),
            AdminTransferredEvent {
                old_admin,
                new_admin,
            },
        );
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if caller != &admin {
            panic!("Not admin");
        }
    }

    /// Combined byte length of every merchant metadata field. Today the only
    /// metadata field is `name`; the helper exists so the limit covers all
    /// fields as more are added.
    fn metadata_len(name: &String) -> u32 {
        name.len()
    }
}
