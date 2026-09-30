#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Env};

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
