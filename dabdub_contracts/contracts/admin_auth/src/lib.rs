#![no_std]
#![deny(warnings)]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec};

/// Contract ABI version. Bumped on every WASM upgrade; CI checks this
/// matches the crate version in Cargo.toml.
pub const VERSION: u32 = 1u32;

/// Shared PascalCase error codes for panic messages (issue #1314).
/// These will be replaced by `#[contracterror]` enums in a future change.
pub mod errors {
    pub const NOT_ADMIN: &str = "NotAdmin";
    pub const ADMIN_NOT_FOUND: &str = "AdminNotFound";
    pub const CANNOT_REVOKE_SUPER_ADMIN: &str = "CannotRevokeSuperAdmin";
}

/// Admin roles. Admin accounts are stored separately from merchant accounts
/// as a deployment convention — the contract itself does not cross-check
/// against a merchant registry, so the separation is upheld by the caller
/// (e.g. the off-chain deployment script) rather than enforced on-chain.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum AdminRole {
    Admin,
    SuperAdmin,
}

/// Separately-credentialed admin account, distinct from the merchant entity.
#[contracttype]
#[derive(Clone, Debug)]
pub struct AdminUser {
    pub admin: Address,
    pub role: AdminRole,
    pub active: bool,
}

/// A single entry in the on-chain admin audit log (#1576).
/// `op_type` is a short PascalCase tag (e.g. "AddAdmin", "RevokeAdmin",
/// "TransferSuperAdmin", "DeactivateAdmin"), `actor` is the authenticated
/// caller, `timestamp` is the ledger timestamp, and `params` is a compact
/// human-readable description of the operation arguments.
#[contracttype]
#[derive(Clone, Debug)]
pub struct AuditEntry {
    pub op_type: soroban_sdk::Symbol,
    pub actor: Address,
    pub timestamp: u64,
    pub params: soroban_sdk::String,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    SuperAdmin,
    Admin(Address),
    AdminIndex,
    Version,
    AuditLog,
}

#[contracttype]
struct AdminAddedEvent {
    admin: Address,
    role: AdminRole,
}

#[contracttype]
struct AdminRevokedEvent {
    admin: Address,
}

#[contracttype]
struct AdminRoleChangedEvent {
    admin: Address,
    old_role: AdminRole,
    new_role: AdminRole,
}

#[contracttype]
struct SuperAdminTransferredEvent {
    previous: Address,
    new: Address,
}

#[contracttype]
struct AdminDeactivatedEvent {
    admin: Address,
}

#[contract]
pub struct AdminAuthContract;

#[contractimpl]
impl AdminAuthContract {
    /// Bootstrap the store with a single SuperAdmin. All other admin accounts
    /// are created exclusively through `add_admin` (the CLI seed/script path).
    pub fn __constructor(env: Env, super_admin: Address) {
        env.storage().instance().set(&DataKey::Version, &VERSION);
        env.storage()
            .instance()
            .set(&DataKey::SuperAdmin, &super_admin);
        Self::save(
            &env,
            AdminUser {
                admin: super_admin.clone(),
                role: AdminRole::SuperAdmin,
                active: true,
            },
        );
        // Initialize the admin index with the super_admin
        let mut index: Vec<Address> = Vec::new(&env);
        index.push_back(super_admin);
        env.storage().instance().set(&DataKey::AdminIndex, &index);
    }

    /// Return the contract ABI version stored at construction time.
    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Version)
            .unwrap_or(VERSION)
    }

    /// Add an admin to the separate credential store. SuperAdmin only.
    /// If the address is already an admin, its role is updated and a distinct
    /// `AdminRoleChangedEvent` is emitted instead of `AdminAddedEvent`, so
    /// off-chain consumers can distinguish new grants from privilege changes.
    pub fn add_admin(env: Env, caller: Address, admin: Address, role: AdminRole) {
        caller.require_auth();
        Self::require_super_admin(&env, &caller);
        let existing: Option<AdminUser> = env
            .storage()
            .persistent()
            .get(&DataKey::Admin(admin.clone()));
        if let Some(ref old_user) = existing {
            // Role update on an existing admin — emit a distinct event.
            Self::save(
                &env,
                AdminUser {
                    admin: admin.clone(),
                    role: role.clone(),
                    active: old_user.active,
                },
            );
            env.events().publish(
                ("ADMIN_AUTH", "admin_role_changed"),
                AdminRoleChangedEvent {
                    admin,
                    old_role: old_user.role.clone(),
                    new_role: role,
                },
            );
            Self::append_audit(
                &env,
                soroban_sdk::Symbol::new(&env, "AdminRoleChanged"),
                &caller,
                soroban_sdk::String::from_str(&env, "role changed"),
            );
        } else {
            // New admin.
            Self::save(
                &env,
                AdminUser {
                    admin: admin.clone(),
                    role: role.clone(),
                    active: true,
                },
            );
            // Add to index only if not already present.
            let mut index: Vec<Address> = env
                .storage()
                .instance()
                .get(&DataKey::AdminIndex)
                .unwrap_or_else(|| Vec::new(&env));
            index.push_back(admin.clone());
            env.storage().instance().set(&DataKey::AdminIndex, &index);
            env.events().publish(
                ("ADMIN_AUTH", "admin_added"),
                AdminAddedEvent { admin, role },
            );
            Self::append_audit(
                &env,
                soroban_sdk::Symbol::new(&env, "AddAdmin"),
                &caller,
                soroban_sdk::String::from_str(&env, "admin added"),
            );
        }
    }

    /// Remove an admin from the credential store. SuperAdmin only.
    /// The SuperAdmin cannot revoke their own admin record (fix for #1041).
    pub fn revoke_admin(env: Env, caller: Address, admin: Address) {
        caller.require_auth();
        Self::require_super_admin(&env, &caller);

        // Prevent the SuperAdmin from revoking themselves (#1041)
        let super_admin: Address = env.storage().instance().get(&DataKey::SuperAdmin).unwrap();
        if admin == super_admin {
            panic!("{}", errors::CANNOT_REVOKE_SUPER_ADMIN);
        }

        let key = DataKey::Admin(admin.clone());
        if !env.storage().persistent().has(&key) {
            panic!("{}", errors::ADMIN_NOT_FOUND);
        }
        env.storage().persistent().remove(&key);

        // Remove from index
        let mut index: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AdminIndex)
            .unwrap_or_else(|| Vec::new(&env));
        if let Some(pos) = index.first_index_of(&admin) {
            index.remove(pos);
            env.storage().instance().set(&DataKey::AdminIndex, &index);
        }

        env.events()
            .publish(("ADMIN_AUTH", "admin_revoked"), AdminRevokedEvent { admin });
        Self::append_audit(
            &env,
            soroban_sdk::Symbol::new(&env, "RevokeAdmin"),
            &caller,
            soroban_sdk::String::from_str(&env, "admin revoked"),
        );
    }

    /// Transfer SuperAdmin rights to a new address. SuperAdmin only (#1042).
    pub fn transfer_super_admin(env: Env, caller: Address, new_super_admin: Address) {
        caller.require_auth();
        Self::require_super_admin(&env, &caller);

        let previous = caller.clone();

        // Update the SuperAdmin key
        env.storage()
            .instance()
            .set(&DataKey::SuperAdmin, &new_super_admin);

        // Ensure the new super admin has an AdminUser record with SuperAdmin role
        let is_new = !env
            .storage()
            .persistent()
            .has(&DataKey::Admin(new_super_admin.clone()));
        Self::save(
            &env,
            AdminUser {
                admin: new_super_admin.clone(),
                role: AdminRole::SuperAdmin,
                active: true,
            },
        );
        if is_new {
            let mut index: Vec<Address> = env
                .storage()
                .instance()
                .get(&DataKey::AdminIndex)
                .unwrap_or_else(|| Vec::new(&env));
            index.push_back(new_super_admin.clone());
            env.storage().instance().set(&DataKey::AdminIndex, &index);
        }

        env.events().publish(
            ("ADMIN_AUTH", "super_admin_transferred"),
            SuperAdminTransferredEvent {
                previous,
                new: new_super_admin,
            },
        );
        Self::append_audit(
            &env,
            soroban_sdk::Symbol::new(&env, "TransferSuperAdmin"),
            &caller,
            soroban_sdk::String::from_str(&env, "super admin transferred"),
        );
    }

    /// Deactivate an admin without removing its credential record. SuperAdmin
    /// only. `is_admin` returns false for a deactivated admin.
    pub fn deactivate_admin(env: Env, caller: Address, admin: Address) {
        caller.require_auth();
        Self::require_super_admin(&env, &caller);

        let key = DataKey::Admin(admin.clone());

