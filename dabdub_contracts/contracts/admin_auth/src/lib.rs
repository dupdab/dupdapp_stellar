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
    /// Caller is not a registered admin (or is deactivated).
    pub const NOT_ADMIN: &str = "NotAdmin";
    /// Target admin address has no credential record in storage.
    pub const ADMIN_NOT_FOUND: &str = "AdminNotFound";
    /// Attempt to revoke the current SuperAdmin's own record.
    pub const CANNOT_REVOKE_SUPER_ADMIN: &str = "CannotRevokeSuperAdmin";
}

/// Admin roles. Admin accounts are stored separately from merchant accounts
/// as a deployment convention — the contract itself does not cross-check
/// against a merchant registry, so the separation is upheld by the caller
/// (e.g. the off-chain deployment script) rather than enforced on-chain.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum AdminRole {
    /// Standard admin: may perform admin-gated operations but cannot manage
    /// other admins.
    Admin,
    /// Super admin: may add/revoke admins, deactivate admins, and transfer
    /// SuperAdmin rights. Exactly one SuperAdmin exists at a time.
    SuperAdmin,
}

/// Separately-credentialed admin account, distinct from the merchant entity.
#[contracttype]
#[derive(Clone, Debug)]
pub struct AdminUser {
    /// The admin's Stellar address.
    pub admin: Address,
    /// Role granted to this admin.
    pub role: AdminRole,
    /// Whether the admin is currently active. Deactivated admins keep their
    /// record but fail `is_admin` checks.
    pub active: bool,
}

/// Storage keys used by the contract.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Instance key holding the current SuperAdmin address.
    SuperAdmin,
    /// Persistent key holding an `AdminUser` record for the given address.
    Admin(Address),
    /// Instance key holding the `Vec<Address>` index of all admin addresses.
    AdminIndex,
    /// Instance key holding the contract ABI version.
    Version,
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
    ///
    /// # Parameters
    /// - `super_admin`: address to register as the initial SuperAdmin. Must be
    ///   a valid Stellar address; no range constraints beyond that.
    ///
    /// # Panics
    /// Does not panic; called once at deployment.
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
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    ///
    /// # Returns
    /// The stored `VERSION` (currently `1`), or `VERSION` if unset.
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
    ///
    /// # Parameters
    /// - `caller`: must be the current SuperAdmin; must authorize the call.
    /// - `admin`: address to add or update. Any valid Stellar address.
    /// - `role`: `AdminRole::Admin` or `AdminRole::SuperAdmin`.
    ///
    /// # Panics
    /// - `NotAdmin` if `caller` is not the SuperAdmin.
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
        }
    }

    /// Remove an admin from the credential store. SuperAdmin only.
    /// The SuperAdmin cannot revoke their own admin record (fix for #1041).
    ///
    /// # Parameters
    /// - `caller`: must be the current SuperAdmin; must authorize the call.
    /// - `admin`: address to revoke. Must exist and must not be the SuperAdmin.
    ///
    /// # Panics
    /// - `NotAdmin` if `caller` is not the SuperAdmin.
    /// - `CannotRevokeSuperAdmin` if `admin` is the current SuperAdmin.
    /// - `AdminNotFound` if `admin` has no credential record.
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
    }

    /// Transfer SuperAdmin rights to a new address. SuperAdmin only (#1042).
    ///
    /// # Parameters
    /// - `caller`: must be the current SuperAdmin; must authorize the call.
    /// - `new_super_admin`: address to become the new SuperAdmin. Any valid
    ///   Stellar address; a credential record is created if absent.
    ///
    /// # Panics
    /// - `NotAdmin` if `caller` is not the SuperAdmin.
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
    }

    /// Deactivate an admin without removing its credential record. SuperAdmin
    /// only. `is_admin` returns false for a deactivated admin.
    ///
    /// # Parameters
    /// - `caller`: must be the current SuperAdmin; must authorize the call.
    /// - `admin`: address to deactivate. Must have an existing credential record.
    ///
    /// # Panics
    /// - `NotAdmin` if `caller` is not the SuperAdmin.
    /// - `AdminNotFound` if `admin` has no credential record.
    pub fn deactivate_admin(env: Env, caller: Address, admin: Address) {
        caller.require_auth();
        Self::require_super_admin(&env, &caller);

        let key = DataKey::Admin(admin.clone()

/* … truncated 1438 chars — edit only what you need near the top … */
