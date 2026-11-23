use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};

fn setup() -> (Env, AdminAuthClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, AdminAuth);
    let client = AdminAuthClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner);
    (env, client, owner)
}

#[test]
fn audit_log_appends_role_changes() {
    let (env, client, _owner) = setup();
    let admin = Address::generate(&env);

    client.add_admin(&admin);
    client.remove_admin(&admin);

    let page = client.get_audit_log(&10, &None);
    assert_eq!(page.entries.len(), 2);
    assert_eq!(page.entries.get(0).unwrap().op_type, Symbol::new(&env, "add_admin"));
    assert_eq!(page.entries.get(1).unwrap().op_type, Symbol::new(&env, "remove_admin"));
    assert_eq!(page.next_cursor, None);
}

#[test]
fn audit_log_records_sensitive_ops() {
    let (env, client, owner) = setup();

    client.record_admin_op(&Symbol::new(&env, "drain"), &Symbol::new(&env, "all"));
    client.record_admin_op(&Symbol::new(&env, "upgrade"), &Symbol::new(&env, "v2"));

    let page = client.get_audit_log(&10, &None);
    assert_eq!(page.entries.len(), 2);
    let drain = page.entries.get(0).unwrap();
    assert_eq!(drain.op_type, Symbol::new(&env, "drain"));
    assert_eq!(drain.actor, owner);
    assert_eq!(drain.params, Symbol::new(&env, "all"));
    assert_eq!(drain.timestamp, env.ledger().timestamp());
}

#[test]
fn audit_log_paginates_with_cursor() {
    let (env, client, _owner) = setup();

    for _ in 0..5 {
        client.record_admin_op(&Symbol::new(&env, "drain"), &Symbol::new(&env, "all"));
    }

    let first = client.get_audit_log(&2, &None);
    assert_eq!(first.entries.len(), 2);
    assert_eq!(first.next_cursor, Some(2));

    let second = client.get_audit_log(&2, &first.next_cursor);
    assert_eq!(second.entries.len(), 2);
    assert_eq!(second.next_cursor, Some(4));

    let third = client.get_audit_log(&2, &second.next_cursor);
    assert_eq!(third.entries.len(), 1);
    assert_eq!(third.next_cursor, None);
}
