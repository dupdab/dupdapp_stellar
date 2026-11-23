#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Events as _, Env, IntoVal, Symbol};

fn setup() -> (Env, AdminAuthContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let super_admin = Address::generate(&env);
    let id = env.register(AdminAuthContract, (&super_admin,));
    let client = AdminAuthContractClient::new(&env, &id);
    (env, client, super_admin)
}

#[test]
fn test_constructor_bootstraps_super_admin() {
    let (env, client, super_admin) = setup();
    let user = client.get_admin(&super_admin).unwrap();
    assert_eq!(user.role, AdminRole::SuperAdmin);
    assert!(user.active);
    assert!(client.is_admin(&super_admin));
}

#[test]
fn test_is_admin_false_for_non_admin() {
    let (env, client, _super_admin) = setup();
    let stranger = Address::generate(&env);
    assert!(!client.is_admin(&stranger));
}

#[test]
fn test_add_admin_grants_role() {
    let (env, client, super_admin) = setup();
    let admin = Address::generate(&env);
    client.add_admin(&super_admin, &admin, &AdminRole::Admin);
    assert!(client.is_admin(&admin));
    let user = client.get_admin(&admin).unwrap();
    assert_eq!(user.role, AdminRole::Admin);
}

#[test]
fn test_authorize_allows_admin() {
    let (env, client, super_admin) = setup();
    let admin = Address::generate(&env);
    client.add_admin(&super_admin, &admin, &AdminRole::Admin);
    client.authorize(&admin);
}

#[test]
#[should_panic(expected = "unauthorized admin")]
fn test_authorize_rejects_merchant_address() {
    let (env, client, _super_admin) = setup();
    let merchant = Address::generate(&env);
    client.authorize(&merchant);
}

#[test]
#[should_panic(expected = "not super admin")]
fn test_add_admin_requires_super_admin() {
    let (env, client, super_admin) = setup();
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    client.add_admin(&super_admin, &admin, &AdminRole::Admin);
    client.add_admin(&attacker, &admin, &AdminRole::Admin);
}

#[test]
fn test_revoke_admin() {
    let (env, client, super_admin) = setup();
    let admin = Address::generate(&env);
    client.add_admin(&super_admin, &admin, &AdminRole::Admin);
    assert!(client.is_admin(&admin));
    client.revoke_admin(&super_admin, &admin);
    assert!(!client.is_admin(&admin));
}

#[test]
#[should_panic(expected = "admin not found")]
fn test_revoke_unknown_admin_panics() {
    let (env, client, super_admin) = setup();
    let stranger = Address::generate(&env);
    client.revoke_admin(&super_admin, &stranger);
}

#[test]
fn test_deactivate_admin() {
    let (env, client, super_admin) = setup();
    let admin = Address::generate(&env);
    client.add_admin(&super_admin, &admin, &AdminRole::Admin);
    assert!(client.is_admin(&admin));
    client.deactivate_admin(&super_admin, &admin);
    assert!(!client.is_admin(&admin));
    let user = client.get_admin(&admin).unwrap();
    assert!(!user.active);
}

#[test]
#[should_panic(expected = "not super admin")]
fn test_deactivate_admin_requires_super_admin() {
    let (env, client, super_admin) = setup();
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    client.add_admin(&super_admin, &admin, &AdminRole::Admin);
    client.deactivate_admin(&attacker, &admin);
}

#[test]
#[should_panic(expected = "admin not found")]
fn test_deactivate_unknown_admin_panics() {
    let (env, client, super_admin) = setup();
    let stranger = Address::generate(&env);
    client.deactivate_admin(&super_admin, &stranger);
}

// Issue #1045 — re-calling add_admin for an existing admin updates role without
// creating a second index entry and preserves the active flag.
#[test]
fn test_add_admin_updates_role_for_existing_admin() {
    let (env, client, super_admin) = setup();
    let admin = Address::generate(&env);

    client.add_admin(&super_admin, &admin, &AdminRole::Admin);
    assert_eq!(client.get_admin(&admin).unwrap().role, AdminRole::Admin);

    // Re-add with a different role — should update, not duplicate.
    client.add_admin(&super_admin, &admin, &AdminRole::SuperAdmin);
    let user = client.get_admin(&admin).unwrap();
    assert_eq!(user.role, AdminRole::SuperAdmin);
    assert!(user.active);

    // The admin should appear exactly once in the index (admin_count unchanged).
    assert_eq!(client.admin_count(), 2); // super_admin + admin
}

// Issue #1046 — get_admin returns None for an address that was never added.
#[test]
fn test_get_admin_returns_none_for_unknown_address() {
    let (env, client, _super_admin) = setup();
    let stranger = Address::generate(&env);
    assert!(client.get_admin(&stranger).is_none());
}

// Issue #1046 — revoke then re-add restores full access.
#[test]
fn test_revoke_then_readd_admin_restores_access() {
    let (env, client, super_admin) = setup();
    let admin = Address::generate(&env);

    client.add_admin(&super_admin, &admin, &AdminRole::Admin);
    assert!(client.is_admin(&admin));

    client.revoke_admin(&super_admin, &admin);
    assert!(!client.is_admin(&admin));
    assert!(client.get_admin(&admin).is_none());

    // Re-add the same address — should be treated as a brand-new admin.
    client.add_admin(&super_admin, &admin, &AdminRole::SuperAdmin);
    assert!(client.is_admin(&admin));
    let user = client.get_admin(&admin).unwrap();
    assert_eq!(user.role, AdminRole::SuperAdmin);
    assert!(user.active);
}

// Issue #1531 — role assignment emits a RoleAssigned event with the expected
// account, role, assigned_by, and timestamp fields for off-chain audit indexing.
#[test]
fn test_add_admin_emits_role_assigned_event() {
    let (env, client, super_admin) = setup();
    let admin = Address::generate(&env);

    client.add_admin(&super_admin, &admin, &AdminRole::Admin);

    let expected = RoleAssigned {
        account: admin.clone(),
        role: AdminRole::Admin,
        assigned_by: super_admin.clone(),
        timestamp: env.ledger().timestamp(),
    };

    let found = env.events().all().iter().any(|(_, topics, data)| {
        topics
            .iter()
            .any(|t| t == Symbol::new(&env, "role_assigned").into_val(&env))
            && data == expected.clone().into_val(&env)
    });
    assert!(found, "RoleAssigned event not emitted");
}

// Issue #1531 — role revocation emits a RoleRevoked event with the expected
// account, role, revoked_by, and timestamp fields for off-chain audit indexing.
#[test]
fn test_revoke_admin_emits_role_revoked_event() {
    let (env, client, super_admin) = setup();
    let admin = Address::generate(&env);

    client.add_admin(&super_admin, &admin, &AdminRole::Admin);
    client.revoke_admin(&super_admin, &admin);

    let expected = RoleRevoked {
        account: admin.clone(),
        role: AdminRole::Admin,
        revoked_by: super_admin.clone(),
        timestamp: env.ledger().timestamp(),
    };

    let found = env.events().all().iter().any(|(_, topics, data)| {
        topics
            .iter()
            .any(|t| t == Symbol::new(&env, "role_revoked").into_val(&env))
            && data == expected.clone().into_val(&env)
    });
    assert!(found, "RoleRevoked event not emitted");
}

// Issue #1532 — role hierarchy: VIEWER < OPERATOR < ADMIN < SUPERADMIN.
// has_permission must check permissions across all roles in the hierarchy up to
// and including the account's role.
#[test]
fn test_has_permission_hierarchy_inherits_lower_roles() {
    let (env, client, super_admin) = setup();
    let admin = Address::generate(&env);
    client.add_admin(&super_admin, &admin, &AdminRole::Admin);

    // ADMIN inherits VIEWER and OPERATOR permissions.
    assert!(client.has_permission(&admin, &Permission::ViewDashboard));
    assert!(client.has_permission(&admin, &Permission::ManagePayments));
    // ADMIN has its own permissions.
    assert!(client.has_permission(&admin, &Permission::ManageAdmins));
    // SUPERADMIN inherits everything.
    assert!(client.has_permission(&super_admin, &Permission::ManageAdmins));
    assert!(client.has_permission(&super_admin, &Permission::SystemConfig));
}

// Issue #1532 — ADMIN cannot perform SUPERADMIN-only actions.
#[test]
fn test_admin_cannot_perform_super_admin_only_actions() {
    let (env, client, super_admin) = setup();
    let admin = Address::generate(&env);
    client.add_admin(&super_admin, &admin, &AdminRole::Admin);

    assert!(!client.has_permission(&admin, &Permission::SystemConfig));
    assert!(!client.has_permission(&admin, &Permission::TransferOwnership));
}

// Issue #1532 — inactive admins have no permissions regardless of role.
#[test]
fn test_inactive_admin_has_no_permissions() {
    let (env, client, super_admin) = setup();
    let admin = Address::generate(&env);
    client.add_admin(&super_admin, &admin, &AdminRole::Admin);
    client.deactivate_admin(&super_admin, &admin);

    assert!(!client.has_permission(&admin, &Permission::ViewDashboard));
}
