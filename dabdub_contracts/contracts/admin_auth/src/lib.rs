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
    pub const UNAUTHORIZED: &str = "Unauthorized";
}

/// Admin roles. Admin accounts are stored separately from merchant accounts
/// as a deployment convention — the contract itself does not cross-check
/// against a merchant registry, so the separation is upheld by the caller
/// (e.g. the off-chain deployment script) rather than enforced on-chain.
///
/// Role hierarchy (issue #1532): VIEWER < OPERATOR < ADMIN < SUPERADMIN.
/// A higher role implicitly holds every permission granted to lower roles.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum AdminRole {
    Viewer,
    Operator,
    Admin,
    SuperAdmin,
}

impl AdminRole {
    /// Numeric rank used to compare roles in the hierarchy.
    pub fn rank(&self) -> u32 {
        match self {
            AdminRole::Viewer => 0,
            AdminRole::Operator => 1,
            AdminRole::Admin => 2,
            AdminRole::SuperAdmin => 3,
        }
    }

    /// True when `self` is at least as privileged as `other`.
    pub fn at_least(&self, other: &AdminRole) -> bool {
        self.rank() >= other.rank()
    }
}

/// Separately-credentialed admin account, distinct from the merchant entity.
#[contracttype]
#[derive(Clone, Debug)]
pub struct AdminUser {
    pub admin: Address,
    pub role: AdminRole,
    pub active: bool,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    SuperAdmin,
    Admin(Address),
    AdminIndex,
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

/// Emitted when a role is assigned to an account (issue #1531).
#[contracttype]
struct RoleAssignedEvent {
    account: Address,
    role: AdminRole,
    assigned_by: Address,
    timestamp: u64,
}

/// Emitted when a role is revoked from an account (issue #1531).
#[contracttype]
struct RoleRevokedEvent {
    account: Address,
    role: AdminRole,
    revoked_by: Address,
    timestamp: u64,
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

    /// Return true when `account` holds `permission` under the role hierarchy
    /// (issue #1532). A role implicitly holds every permission granted to the
    /// roles below it: VIEWER < OPERATOR < ADMIN < SUPERADMIN.
    pub fn has_permission(env: Env, account: Address, permission: AdminRole) -> bool {
        match Self::get_admin(&env, &account) {
            Some(user) if user.active => user.role.at_least(&permission),
            _ => false,
        }
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
                    admin: admin.clone(),
                    old_role: old_user.role.clone(),
                    new_role: role.clone(),
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
                AdminAddedEvent {
                    admin: admin.clone(),
                    role: role.clone(),
                },
            );
        }

        // Audit trail: emit a role-assignment event for off-chain monitoring.
        env.events().publish(
            ("ADMIN_AUTH", "role_assigned"),
            RoleAssignedEvent {
                account: admin,
                role,
                assigned_by: caller,
                timestamp: env.ledger().timestamp(),
            },
        );
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
        let revoked_role = env
            .storage()
            .persistent()
            .get::<DataKey, AdminUser>(&key)
            .map(|u| u.role)
            .unwrap_or(AdminRole::Admin);
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

        env.events().publish(
            ("ADMIN_AUTH", "admin_revoked"),
            AdminRevokedEvent {
                admin: admin.clone(),
            },
        );

        // Audit trail: emit a role-revocation event for off-chain monitoring.
        env.events().publish(
            ("ADMIN_AUTH", "role_revoked"),
            RoleRevokedEvent {
                account: admin,
                role: revoked_role,
                revoked_by: caller,
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    /// Transfer SuperAdmin rights to a new address. SuperAdmin only (#1042).
    pub fn transfer_super_admin(env: Env, caller: Address, new_super_admin: Address) {
        caller.require_auth();
        Self::require_super_admin(&env, &caller);

        let previous = caller.clone();

        // Demote the previous SuperAdmin to Admin so they retain admin rights
        // but no longer hold SuperAdmin privileges.
        Self::save(
            &env,
            AdminUser {
                admin: previous.clone(),
                role: AdminRole::Admin,
                active: true,
            },
        );

        // Promote the new SuperAdmin.
        Self::save(
            &env,
            AdminUser {
                admin: new_super_admin.clone(),
                role: AdminRole::SuperAdmin,
                active: true,
            },
        );

        // Ensure the new SuperAdmin is tracked in the index.
        let mut index: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AdminIndex)
            .unwrap_or_else(|| Vec::new(&env));
        if index.first_index_of(&new_super_admin).is_none() {
            index.push_back(new_super_admin.clone());
            env.storage().instance().set(&DataKey::AdminIndex, &index);
        }

        env.storage()
            .instance()
            .set(&DataKey::SuperAdmin, &new_super_admin);

        env.events().publish(
            ("ADMIN_AUTH", "super_admin_transferred"),
            SuperAdminTransferredEvent {
                previous,
                new: new_super_admin,
            },
        );
    }

    /// Deactivate an admin without removing their record. SuperAdmin only.
    pub fn deactivate_admin(env: Env, caller: Address, admin: Address) {
        caller.require_auth();
        Self::require_super_admin(&env, &caller);

        let key = DataKey::Admin(admin.clone());
        let mut user: AdminUser = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic!("{}", errors::ADMIN_NOT_FOUND));
        user.active = false;
        Self::save(&env, user);

        env.events().publish(
            ("ADMIN_AUTH", "admin_deactivated"),
            AdminDeactivatedEvent {
                admin: admin.clone(),
            },
        );
    }

    /// Return the stored admin record for `account`, if any.
    pub fn get_admin(env: &Env, account: &Address) -> Option<AdminUser> {
        env.storage()
            .persistent()
            .get(&DataKey::Admin(account.clone()))
    }

    /// Return true when `account` is an active admin of at least `role`.
    pub fn is_admin(env: Env, account: Address, role: AdminRole) -> bool {
        match Self::get_admin(&env, &account) {
            Some(user) if user.active => user.role.at_least(&role),
            _ => false,
        }
    }

    fn save(env: &Env, user: AdminUser) {
        env.storage()
            .persistent()
            .set(&DataKey::Admin(user.admin.clone()), &user);
    }

    fn require_super_admin(env: &Env, caller: &Address) {
        let super_admin: Address = env.storage().instance().get(&DataKey::SuperAdmin).unwrap();
        if caller != &super_admin {
            panic!("{}", errors::NOT_ADMIN);
        }
    }
}
