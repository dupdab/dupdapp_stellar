#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, Vec,
};

fn setup_env() -> (Env, Address, AdminTimelockContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(AdminTimelockContract, (&admin, &DEFAULT_MIN_DELAY_SECONDS));
    let client = AdminTimelockContractClient::new(&env, &contract_id);

    (env, admin, client)
}

fn make_id(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

#[test]
fn test_queue_action_applies_minimum_delay() {
    let (env, admin, client) = setup_env();
    let action_id = make_id(&env, 1);

    let queued = client.queue_action(&admin, &action_id);
    assert_eq!(queued.action_id, action_id);
    assert_eq!(client.get_min_delay(), DEFAULT_MIN_DELAY_SECONDS);
    assert!(client.get_pending_action(&action_id).is_some());
}

#[test]
fn test_execute_action_requires_delay_to_elapse() {
    let (env, admin, client) = setup_env();
    let action_id = make_id(&env, 2);
    client.queue_action(&admin, &action_id);

    env.ledger().with_mut(|li| li.timestamp += DEFAULT_MIN_DELAY_SECONDS / 2);
    assert!(client.try_execute_action(&admin, &action_id).is_err());

    env.ledger().with_mut(|li| li.timestamp += DEFAULT_MIN_DELAY_SECONDS);
    client.execute_action(&admin, &action_id);
    assert!(client.get_pending_action(&action_id).is_none());
}

#[test]
fn test_cancel_timelock_removes_pending_action() {
    let (env, admin, client) = setup_env();
    let action_id = make_id(&env, 3);
    client.queue_action(&admin, &action_id);

    client.cancel_timelock(&admin, &action_id);
    assert!(client.get_pending_action(&action_id).is_none());
}

#[test]
#[should_panic(expected = "timelock has not elapsed")]
fn test_execute_before_delay_panics() {
    let (env, admin, client) = setup_env();
    let action_id = make_id(&env, 4);
    client.queue_action(&admin, &action_id);
    client.execute_action(&admin, &action_id);
}

#[test]
fn test_execute_within_execution_window_succeeds() {
    let (env, admin, client) = setup_env();
    let action_id = make_id(&env, 5);
    client.queue_action(&admin, &action_id);

    // Advance past the timelock delay but stay within the execution window.
    env.ledger().with_mut(|li| li.timestamp += DEFAULT_MIN_DELAY_SECONDS + 1);
    client.execute_action(&admin, &action_id);
    assert!(client.get_pending_action(&action_id).is_none());
}

#[test]
fn test_execute_after_execution_window_expires() {
    let (env, admin, client) = setup_env();
    let action_id = make_id(&env, 6);
    client.queue_action(&admin, &action_id);

    // Advance past the timelock delay plus the full execution window.
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_MIN_DELAY_SECONDS + MAX_EXECUTION_WINDOW_SECONDS + 1
    });

    let result = client.try_execute_action(&admin, &action_id);
    assert_eq!(result, Err(Ok(Error::OperationExpired)));
    // Expired operation is auto-cancelled and removed from pending state.
    assert!(client.get_pending_action(&action_id).is_none());
}

#[test]
fn test_queue_and_execute_batch_of_three() {
    let (env, admin, client) = setup_env();
    let batch_id = make_id(&env, 7);

    let mut ops: Vec<BytesN<32>> = Vec::new(&env);
    ops.push_back(make_id(&env, 10));
    ops.push_back(make_id(&env, 11));
    ops.push_back(make_id(&env, 12));

    let queued = client.queue_batch(&admin, &batch_id, &ops);
    assert_eq!(queued.action_id, batch_id);
    assert_eq!(queued.operations.len(), 3);
    assert!(client.get_pending_action(&batch_id).is_some());

    env.ledger().with_mut(|li| li.timestamp += DEFAULT_MIN_DELAY_SECONDS + 1);
    client.execute_batch(&admin, &batch_id);

    // All operations in the batch executed atomically and the entry is cleared.
    assert!(client.get_pending_action(&batch_id).is_none());
    assert!(client.is_operation_executed(&make_id(&env, 10)));
    assert!(client.is_operation_executed(&make_id(&env, 11)));
    assert!(client.is_operation_executed(&make_id(&env, 12)));
}

#[test]
fn test_batch_execution_is_atomic_all_or_nothing() {
    let (env, admin, client) = setup_env();
    let batch_id = make_id(&env, 8);

    let mut ops: Vec<BytesN<32>> = Vec::new(&env);
    ops.push_back(make_id(&env, 20));
    ops.push_back(make_id(&env, 21));
    ops.push_back(make_id(&env, 22));

    client.queue_batch(&admin, &batch_id, &ops);

    // Attempting to execute before the delay elapses must fail and leave the
    // batch (and every operation in it) unexecuted.
    assert!(client.try_execute_batch(&admin, &batch_id).is_err());
    assert!(client.get_pending_action(&batch_id).is_some());
    assert!(!client.is_operation_executed(&make_id(&env, 20)));
    assert!(!client.is_operation_executed(&make_id(&env, 21)));
    assert!(!client.is_operation_executed(&make_id(&env, 22)));

    // Once the delay elapses the whole batch executes together.
    env.ledger().with_mut(|li| li.timestamp += DEFAULT_MIN_DELAY_SECONDS + 1);
    client.execute_batch(&admin, &batch_id);
    assert!(client.is_operation_executed(&make_id(&env, 20)));
    assert!(client.is_operation_executed(&make_id(&env, 21)));
    assert!(client.is_operation_executed(&make_id(&env, 22)));
}
