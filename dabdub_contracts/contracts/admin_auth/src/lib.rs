#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec};

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
    AdminIndex,
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
        env.storage().instance().set(&DataKey::SuperAdmin, &super_admin);
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

    /// Add an admin to the separate credential store. SuperAdmin only.
    pub fn add_admin(env: Env, caller: Address, admin: Address, role: AdminRole) {
        caller.require_auth();
        Self::require_super_admin(&env, &caller);
        let is_new = !env.storage().persistent().has(&DataKey::Admin(admin.clone()));
        Self::save(
            &env,
            AdminUser {
                admin: admin.clone(),
                role: role.clone(),
                active: true,
            },
        );
        // Add to index only if not already present
        if is_new {
            let mut index: Vec<Address> = env
                .storage()
                .instance()
                .get(&DataKey::AdminIndex)
                .unwrap_or_else(|| Vec::new(&env));
            index.push_back(admin.clone());
            env.storage().instance().set(&DataKey::AdminIndex, &index);
        }
        env.events().publish(
            ("ADMIN_AUTH", "admin_added"),
            AdminAddedEvent {
                admin,
                role,
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
            panic!("cannot revoke super admin");
        }

        let key = DataKey::Admin(admin.clone());
        if !env.storage().persistent().has(&key) {
            panic!("admin not found");
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

        env.events().publish(
            ("ADMIN_AUTH", "admin_revoked"),
            AdminRevokedEvent { admin },
        );
    }

    /// Transfer SuperAdmin rights to a new address. SuperAdmin only (#1042).
    pub fn transfer_super_admin(env: Env, caller: Address, new_super_admin: Address) {
        caller.require_auth();
        Self::require_super_admin(&env, &caller);

        let previous = caller.clone();

        // Update the SuperAdmin key
        env.storage().instance().set(&DataKey::SuperAdmin, &new_super_admin);

        // Ensure the new super admin has an AdminUser record with SuperAdmin role
        let is_new = !env.storage().persistent().has(&DataKey::Admin(new_super_admin.clone()));
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
        env.storage().persistent().get(&DataKey::Admin(admin))
    }

    /// True when the address is an active admin in the store.
    pub fn is_admin(env: Env, admin: Address) -> bool {
        match env.storage().persistent().get::<DataKey, AdminUser>(&DataKey::Admin(admin)) {
            Some(user) => user.active,
            None => false,
        }
    }

    /// Authorization gate for admin-only oper
        );
    }

    pub fn get_admin(env: Env, admin: Address) -> Option<AdminUser> {
        env.storage().persistent().get(&DataKey::Admin(admin))
    }

    /// True when the address is an active admin in the store.
    pub fn is_admin(env: Env, admin: Address) -> bool {
        match env.storage().persistent().get::<DataKey, AdminUser>(&DataKey::Admin(admin)) {
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

    /// Paginated list of current admin addresses (#1044).
    /// Returns up to `page_size` entries starting at `offset`.
    pub fn list_admins(env: Env, offset: u32, page_size: u32) -> Vec<Address> {
        let index: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AdminIndex)
            .unwrap_or_else(|| Vec::new(&env));
        let len = index.len();
        let start = offset.min(len);
        let end = (start + page_size).min(len);
        let mut page: Vec<Address> = Vec::new(&env);
        for i in start..end {
            page.push_back(index.get(i).unwrap());
        }
        page
    }

    /// Total number of admins currently registered.
    pub fn admin_count(env: Env) -> u32 {
        let index: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AdminIndex)
            .unwrap_or_else(|| Vec::new(&env));
        index.len()
    }

    fn save(env: &Env, user: AdminUser) {
        // Use persistent() storage so each admin entry has its own TTL (#1043)
        env.storage()
            .persistent()
            .set(&DataKey::Admin(user.admin.clone()), &user);
    }

    fn require_super_admin(env: &Env, caller: &Address) {
        let super_admin: Address = env.storage().instance().get(&DataKey::SuperAdmin).unwrap();
        if caller != &super_admin {
            panic!("not super admin");
        }
    }
}
