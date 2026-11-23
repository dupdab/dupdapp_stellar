#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::{Address as _, Events}, Env, IntoVal, Symbol};

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

// Issue #1554 — end-to-end admin key rotation: old key works, rotate, new key
// works, and the old key is rejected afterwards.
#[test]
fn test_admin_key_rotation_end_to_end() {
    let (env, client, super_admin) = setup();
    let old_key = Address::generate(&env);
    let new_key = Address::generate(&env);

    // Old key is a valid admin and can perform an admin action.
    client.add_admin(&super_admin, &old_key, &AdminRole::Admin);
    assert!(client.is_admin(&old_key));
    client.authorize(&old_key);

    // Rotate: revoke the old key and grant the new key.
    client.revoke_admin(&super_admin, &old_key);
    client.add_admin(&super_admin, &new_key, &AdminRole::Admin);

    // New key can perform an admin action.
    assert!(client.is_admin(&new_key));
    client.authorize(&new_key);

    // Old key is rejected after rotation.
    assert!(!client.is_admin(&old_key));
    assert!(client.get_admin(&old_key).is_none());
}

#[test]
#[should_panic(expected = "unauthorized admin")]
fn test_old_key_rejected_after_rotation() {
    let (env, client, super_admin) = setup();
    let old_key = Address::generate(&env);
    let new_key = Address::generate(&env);

    client.add_admin(&super_admin, &old_key, &AdminRole::Admin);
    client.authorize(&old_key);

    client.revoke_admin(&super_admin, &old_key);
    client.add_admin(&super_admin, &new_key, &AdminRole::Admin);

    // Old key must no longer be authorized after rotation.
    client.authorize(&old_key);
}

// Issue #1555 — a failed admin authentication must emit an AdminAuthFailed
// event carrying the caller, the attempted function name, and the ledger
// timestamp so brute-force attempts are visible on-chain.
#[test]
fn test_admin_auth_failure_emits_event() {
    let (env, client, _super_admin) = setup();
    let attacker = Address::generate(&env);

    // Record the ledger timestamp the failure will be stamped with.
    let timestamp = env.ledger().timestamp();

    // Attempt an admin-only action from a non-admin address. The contract
    // emits the failure event before panicking, so catch the panic.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.authorize(&attacker);
    }));
    assert!(result.is_err(), "expected auth failure to panic");

    // The failure event must be present in the emitted events.
    let expected = AdminAuthFailed {
        caller: attacker.clone(),
        function: Symbol::new(&env, "authorize"),
        timestamp,
    };
    let expected_topics = (Symbol::new(&env, "admin_auth"), Symbol::new(&env, "auth_failed")).into_val(&env);
    let expected_data = expected.into_val(&env);
    assert!(env.events().all().iter().any(|(_, topics, data)| {
        topics == expected_topics && data == expected_data
    }));
}

// Issue #1556 — admin credentials are stored as derived keys (a hash of the
// public key) rather than the raw public key. The derived key must be stable
// and must not equal the raw address, so credential rotation can be reasoned
// about without exposing the raw key in storage.
#[test]
fn test_admin_credential_is_derived_not_raw() {
    let (env, client, super_admin) = setup();
    let admin = Address::generate(&env);
    client.add_admin(&super_admin, &admin, &AdminRole::Admin);

    let derived = client.derive_admin_key(&admin);
    // The stored credential is a derivation of the public key, not the key itself.
    assert_ne!(derived, admin);
    // Derivation is deterministic.
    assert_eq!(derived, client.derive_admin_key(&admin));
    // The derived credential is what the contract stores and checks.
    assert!(client.has_admin_key(&derived));
    assert!(!client.has_admin_key(&admin));
}

// Issue #1556 — multiple admin keys can be active simultaneously so a new key
// can be added before an old one is removed, enabling rotation without downtime.
#[test]
fn test_multiple_admin_keys_active_during_rotation() {
    let (env, client, super_admin) = setup();
    let old_key = Address::generate(&env);
    let new_key = Address::generate(&env);

    // Grant the old key.
    client.add_admin(&super_admin, &old_key, &AdminRole::Admin);
    assert!(client.is_admin(&old_key));

    // Add the new key BEFORE removing the old one — both must be authorized.
    client.add_admin(&super_admin, &new_key, &AdminRole::Admin);
    assert!(client.is_admin(&old_key));
    assert!(client.is_admin(&new_key));
    client.authorize(&old_key);
    client.authorize(&new_key);

    // Now retire the old key; the new key remains authorized.
    client.revoke_admin(&super_admin, &old_key);
    assert!(!client.is_admin(&old_key));
    assert!(client.is_admin(&new_key));
    client.authorize(&new_key);
}

// Issue #1556 — a removed key's derived credential is no longer recognized.
#[test]
fn test_removed_admin_key_credential_rejected() {
    let (env, client, super_admin) = setup();
    let admin = Address::generate(&env);
    client.add_admin(&super_admin, &admin, &AdminRole::Admin);

    let derived = client.derive_admin_key(&admin);
    assert!(client.has_admin_key(&derived));

    client.revoke_admin(&super_admin, &admin);
    assert!(!client.has_admin_key(&derived));
}
