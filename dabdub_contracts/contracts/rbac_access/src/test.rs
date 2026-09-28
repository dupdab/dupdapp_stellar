#![cfg(test)]

use crate::{RbacAccessContract, RbacAccessContractClient, Role};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup_env() -> (
    Env,
    RbacAccessContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let super_admin = Address::generate(&env);
    let operations_admin = Address::generate(&env);
    let compliance_admin = Address::generate(&env);
    let read_only = Address::generate(&env);

    let contract_id = env.register(RbacAccessContract, (&super_admin,));
    let client = RbacAccessContractClient::new(&env, &contract_id);

    (env, client, super_admin, operations_admin, compliance_admin, read_only)
}

#[test]
fn test_super_admin_can_delegate_without_losing_own_access() {
    let (_env, client, super_admin, operations_admin, _compliance_admin, _read_only) = setup_env();

    client.grant_role(&super_admin, &operations_admin, &Role::OperationsAdmin);
    assert_eq!(
        client.get_role(&operations_admin),
        Some(Role::OperationsAdmin)
    );

    // SuperAdmin retains full access even after delegation.
    client.execute_operations_task(&super_admin);
    client.execute_compliance_task(&super_admin);
    client.execute_read_task(&super_admin);
}

#[test]
fn test_each_role_enforces_minimum_required_permission() {
    let (_env, client, super_admin, operations_admin, compliance_admin, read_only) = setup_env();

    client.grant_role(&super_admin, &operations_admin, &Role::OperationsAdmin);
    client.grant_role(&super_admin, &compliance_admin, &Role::ComplianceAdmin);
    client.grant_role(&super_admin, &read_only, &Role::ReadOnly);

    // OperationsAdmin can do operations + lower-level tasks.
    client.execute_operations_task(&operations_admin);
    client.execute_compliance_task(&operations_admin);
    client.execute_read_task(&operations_admin);

    // ComplianceAdmin can do compliance + read, but not operations.
    client.execute_compliance_task(&compliance_admin);
    client.execute_read_task(&compliance_admin);

    // ReadOnly can only do read task.
    client.execute_read_task(&read_only);
}

#[test]
#[should_panic(expected = "insufficient role")]
fn test_compliance_admin_cannot_execute_operations_task() {
    let (_env, client, super_admin, _operations_admin, compliance_admin, _read_only) = setup_env();
    client.grant_role(&super_admin, &compliance_admin, &Role::ComplianceAdmin);
    client.execute_operations_task(&compliance_admin);
}

#[test]
#[should_panic(expected = "insufficient role")]
fn test_read_only_cannot_execute_compliance_task() {
    let (_env, client, super_admin, _operations_admin, _compliance_admin, read_only) = setup_env();
    client.grant_role(&super_admin, &read_only, &Role::ReadOnly);
    client.execute_compliance_task(&read_only);
}

#[test]
#[should_panic(expected = "insufficient role")]
fn test_non_super_admin_cannot_grant_role() {
    let (_env, client, super_admin, operations_admin, compliance_admin, _read_only) = setup_env();
    client.grant_role(&super_admin, &operations_admin, &Role::OperationsAdmin);
    client.grant_role(&operations_admin, &compliance_admin, &Role::ReadOnly);
}

#[test]
fn test_super_admin_can_revoke_role() {
    let (_env, client, super_admin, _operations_admin, compliance_admin, _read_only) = setup_env();
    client.grant_role(&super_admin, &compliance_admin, &Role::ComplianceAdmin);
    assert_eq!(
        client.get_role(&compliance_admin),
        Some(Role::ComplianceAdmin)
    );

    client.revoke_role(&super_admin, &compliance_admin);
    assert_eq!(client.get_role(&compliance_admin), None);
}

#[test]
fn test_super_admin_count_tracks_grant_revoke_and_transfer() {
    let (env, client, super_admin, operations_admin, compliance_admin, _read_only) = setup_env();
    assert_eq!(client.get_super_admin_count(), 1);
    client.grant_role(&super_admin, &operations_admin, &Role::SuperAdmin);
    assert_eq!(client.get_super_admin_count(), 2);
    client.revoke_role(&super_admin, &operations_admin);
    assert_eq!(client.get_super_admin_count(), 1);
    client.transfer_super_admin(&super_admin, &compliance_admin);
    assert_eq!(client.get_super_admin_count(), 1);
    assert_eq!(client.get_role(&compliance_admin), Some(Role::SuperAdmin));
    let _ = env;
}
