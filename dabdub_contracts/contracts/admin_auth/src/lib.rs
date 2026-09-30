#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

/// Admin roles. Admin accounts are stored separately from merchant accounts,
/// so a merchant JWT/address is never accepted on admin-gated operations.
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

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    SuperAdmin,
    Admin(Address),
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
        env.storage().instance().set(&DataKey::SuperAdmin, &super_admin);
        Self::save(
            &env,
            AdminUser {
                admin: super_admin.clone(),
                role: AdminRole::SuperAdmin,
                active: true,
            },
        );
    }

    /// Add an admin to the separate credential store. SuperAdmin only.
    pub fn add_admin(env: Env, caller: Address, admin: Address, role: AdminRole) {
        caller.require_auth();
        Self::require_super_admin(&env, &caller);
        Self::save(
            &env,
            AdminUser {
                admin: admin.clone(),
                role: role.clone(),
                active: true,
            },
        );
        env.events().publish(
            ("ADMIN_AUTH", "admin_added"),
            AdminAddedEvent {
                admin,
                role,
            },
        );
    }

    /// Remove an admin from the credential store. SuperAdmin only.
    pub fn revoke_admin(env: Env, caller: Address, admin: Address) {
        caller.require_auth();
        Self::require_super_admin(&env, &caller);

        let key = DataKey::Admin(admin.clone());
        if !env.storage().instance().has(&key) {
            panic!("admin not found");
        }
        env.storage().instance().remove(&key);
        env.events().publish(
            ("ADMIN_AUTH", "admin_revoked"),
            AdminRevokedEvent { admin },
        );
    }

    /// Deactivate an admin without removing its credential record. SuperAdmin
    /// only. `is_admin` returns false for a deactivated admin.
    pub fn deactivate_admin(env: Env, caller: Address, admin: Address) {
        caller.require_auth();
        Self::require_super_admin(&env, &caller);

        let key = DataKey::Admin(admin.clone());
        let mut user: AdminUser = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("admin not found"));
        user.active = false;
        Self::save(&env, user);
        env.events().publish(
            ("ADMIN_AUTH", "admin_deactivated"),
            AdminDeactivatedEvent { admin },
        );
    }

    pub fn get_admin(env: Env, admin: Address) -> Option<AdminUser> {
        env.storage().instance().get(&DataKey::Admin(admin))
    }

    /// True when the address is an active admin in the store.
    pub fn is_admin(env: Env, admin: Address) -> bool {
        match env.storage().instance().get::<DataKey, AdminUser>(&DataKey::Admin(admin)) {
            Some(user) => user.active,
            None => false,
        }
    }

    /// Authorization gate for admin-only operations. Rejects any non-admin
    /// address (including merchants), mirroring "require admin JWT on admin routes".
    pub fn authorize(env: Env, caller: Address) {
        caller.require_auth();
        if !Self::is_admin(env.clone(), caller) {
            panic!("unauthorized admin");
        }
    }

    fn save(env: &Env, user: AdminUser) {
        env.storage()
            .instance()
            .set(&DataKey::Admin(user.admin.clone()), &user);
    }

    fn require_super_admin(env: &Env, caller: &Address) {
        let super_admin: Address = env.storage().instance().get(&DataKey::SuperAdmin).unwrap();
        if caller != &super_admin {
            panic!("not super admin");
        }
    }
}
