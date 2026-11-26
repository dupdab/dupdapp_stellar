#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, String};

fn setup() -> (Env, ContractRegistryClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ContractRegistry);
    let client = ContractRegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, client, admin)
}

#[test]
fn test_initialize_sets_admin() {
    let (_env, client, admin) = setup();
    assert_eq!(client.get_admin(), admin);
}

#[test]
fn test_admin_can_set_and_get_address() {
    let (env, client, _admin) = setup();
    let target = Address::generate(&env);
    let name = String::from_str(&env, "payment_escrow");

    client.set_address(&name, &target);

    assert_eq!(client.get_address(&name), Some(target.clone()));
    assert!(client.has_address(&name));
}

#[test]
fn test_admin_can_update_address() {
    let (env, client, _admin) = setup();
    let name = String::from_str(&env, "fee_calculator");
    let first = Address::generate(&env);
    let second = Address::generate(&env);

    client.set_address(&name, &first);
    client.set_address(&name, &second);

    assert_eq!(client.get_address(&name), Some(second));
}

#[test]
fn test_get_missing_address_returns_none() {
    let (env, client, _admin) = setup();
    let name = String::from_str(&env, "unknown");
    assert_eq!(client.get_address(&name), None);
    assert!(!client.has_address(&name));
}

#[test]
fn test_non_admin_cannot_set_address() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ContractRegistry);
    let client = ContractRegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let name = String::from_str(&env, "payment_escrow");
    let target = Address::generate(&env);

    // No auth mocked: the admin auth requirement must fail.
    let result = client.try_set_address(&name, &target);
    assert!(result.is_err());
    assert_eq!(client.get_address(&name), None);
}

#[test]
fn test_stale_address_detection() {
    let (env, client, _admin) = setup();
    let name = String::from_str(&env, "payment_escrow");
    let old_addr = Address::generate(&env);
    let new_addr = Address::generate(&env);

    client.set_address(&name, &old_addr);
    let resolved_before = client.get_address(&name).unwrap();
    assert_eq!(resolved_before, old_addr);

    // Admin redeploys and updates the registry entry.
    client.set_address(&name, &new_addr);

    let resolved_after = client.get_address(&name).unwrap();
    assert_eq!(resolved_after, new_addr);
    assert_ne!(resolved_after, old_addr);
    assert!(client.is_stale(&name, &old_addr));
    assert!(!client.is_stale(&name, &new_addr));
}

#[test]
fn test_is_stale_for_missing_entry() {
    let (env, client, _admin) = setup();
    let name = String::from_str(&env, "missing");
    let addr = Address::generate(&env);
    assert!(client.is_stale(&name, &addr));
}

#[test]
fn test_remove_address() {
    let (env, client, _admin) = setup();
    let name = String::from_str(&env, "payment_escrow");
    let target = Address::generate(&env);

    client.set_address(&name, &target);
    assert!(client.has_address(&name));

    client.remove_address(&name);
    assert!(!client.has_address(&name));
    assert_eq!(client.get_address(&name), None);
}

#[test]
fn test_symbol_short_name_supported() {
    let (env, client, _admin) = setup();
    let target = Address::generate(&env);
    let name = String::from_str(&env, "registry");
    let _ = symbol_short!("registry");

    client.set_address(&name, &target);
    assert_eq!(client.get_address(&name), Some(target));
}
