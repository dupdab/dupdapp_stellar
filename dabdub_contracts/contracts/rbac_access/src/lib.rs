#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    ReadOnly,
    ComplianceAdmin,
    OperationsAdmin,
    SuperAdmin,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Role(Address),
    SuperAdminCount,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RoleGrantedEvent {
    pub account: Address,
    pub role: Role,
    pub granted_by: Address,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RoleRevokedEvent {
    pub account: Address,
    pub revoked_by: Address,
}

#[contract]
pub struct RbacAccessContract;

#[contractimpl]
impl RbacAccessContract {
    pub fn __constructor(env: Env, super_admin: Address) {
        // Issue #1026: Store roles in persistent() instead of instance()
        env.storage()
            .persistent()
            .set(&DataKey::Role(super_admin), &Role::SuperAdmin);
        env.storage()
            .persistent()
            .set(&DataKey::SuperAdminCount, &1u32);
    }

    pub fn grant_role(env: Env, caller: Address, account: Address, role: Role) {
        caller.require_auth();
        Self::require_role(&env, &caller, Role::SuperAdmin);

        let is_new_super_admin = role == Role::SuperAdmin
            && !env
                .storage()
                .persistent()
                .has(&DataKey::Role(account.clone()));

        env.storage()
            .persistent()
            .set(&DataKey::Role(account.clone()), &role);

        if is_new_super_admin {
            let count: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::SuperAdminCount)
                .unwrap_or(0);
            env.storage()
                .persistent()
                .set(&DataKey::SuperAdminCount, &(count + 1));
        }

        env.events().publish(
            ("RBAC", "role_granted"),
            RoleGrantedEvent {
                account,
                role,
                granted_by: caller,
            },
        );
    }

    pub fn revoke_role(env: Env, caller: Address, account: Address) {
        caller.require_auth();
        Self::require_role(&env, &caller, Role::SuperAdmin);

        if caller == account {
            panic!("cannot revoke own role");
        }

        let key = DataKey::Role(account.clone());
        if !env.storage().persistent().has(&key) {
            panic!("role not assigned");
        }

        let role: Role = env
            .storage()
            .persistent()
            .get(&key)
            .expect("role not assigned");

        if role == Role::SuperAdmin {
            let count: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::SuperAdminCount)
                .unwrap_or(0);
            if count <= 1 {
                panic!("cannot revoke last super admin");
            }
            env.storage()
                .persistent()
                .set(&DataKey::SuperAdminCount, &(count - 1));
        }

        env.storage().persistent().remove(&key);
        env.events().publish(
            ("RBAC", "role_revoked"),
            RoleRevokedEvent {
                account,
                revoked_by: caller,
            },
        );
    }

    pub fn get_role(env: Env, account: Address) -> Option<Role> {
        env.storage().persistent().get(&DataKey::Role(account))
    }

    /// Sensitive operation requiring minimum `OperationsAdmin`.
    pub fn execute_operations_task(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_role(&env, &caller, Role::OperationsAdmin);
    }

    /// Sensitive operation requiring minimum `ComplianceAdmin`.
    pub fn execute_compliance_task(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_role(&env, &caller, Role::ComplianceAdmin);
    }

    /// Sensitive operation requiring minimum `ReadOnly`.
    pub fn execute_read_task(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_role(&env, &caller, Role::ReadOnly);
    }

    pub fn transfer_super_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        Self::require_role(&env, &caller, Role::SuperAdmin);

        if caller == new_admin {
            panic!("cannot transfer to self");
        }

        let caller_key = DataKey::Role(caller.clone());
        if !env.storage().persistent().has(&caller_key) {
            panic!("caller is not an admin");
        }

        env.storage()
            .persistent()
            .set(&DataKey::Role(new_admin.clone()), &Role::SuperAdmin);
        env.storage().persistent().remove(&caller_key);

        env.events().publish(
            ("RBAC", "super_admin_transferred"),
            RoleGrantedEvent {
                account: new_admin,
                role: Role::SuperAdmin,
                granted_by: caller,
            },
        );
    }

    fn require_role(env: &Env, caller: &Address, minimum_role: Role) {
        // Issue #1026: Use persistent() instead of instance()
        let caller_role = env
            .storage()
            .persistent()
            .get::<DataKey, Role>(&DataKey::Role(caller.clone()))
            .expect("role not assigned");

        if Self::role_rank(caller_role) < Self::role_rank(minimum_role) {
            panic!("insufficient role");
        }
    }

    fn role_rank(role: Role) -> u32 {
        match role {
            Role::ReadOnly => 1,
            Role::ComplianceAdmin => 2,
            Role::OperationsAdmin => 3,
            Role::SuperAdmin => 4,
        }
    }
}
