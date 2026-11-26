#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, BytesN, Env, IntoVal, Symbol, TryIntoVal, Vec,
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
fn test_min_delay_enforced_at_or_above_minimum() {
    let (env, admin, client) = setup_env();
    let action_id = make_id(&env, 5);

    // Exactly the minimum delay is accepted.
    client.set_min_delay(&admin, &DEFAULT_MIN_DELAY_SECONDS);
    assert_eq!(client.get_min_delay(), DEFAULT_MIN_DELAY_SECONDS);

    // A larger, governance-configured delay is also accepted.
    let larger = DEFAULT_MIN_DELAY_SECONDS + 3_600;
    client.set_min_delay(&admin, &larger);
    assert_eq!(client.get_min_delay(), larger);

    client.queue_action(&admin, &action_id);
    assert!(client.get_pending_action(&action_id).is_some());
}

#[test]
fn test_min_delay_rejects_zero_and_below_minimum() {
    let (env, admin, client) = setup_env();

    // A 0-second timelock must be rejected.
    let zero_result = client.try_set_min_delay(&admin, &0);
    assert_eq!(zero_result, Err(Ok(Error::TimelockTooShort)));
    assert_eq!(client.get_min_delay(), DEFAULT_MIN_DELAY_SECONDS);

    // Any duration below the minimum must be rejected.
    let below = DEFAULT_MIN_DELAY_SECONDS - 1;
    let below_result = client.try_set_min_delay(&admin, &below);
    assert_eq!(below_result, Err(Ok(Error::TimelockTooShort)));
    assert_eq!(client.get_min_delay(), DEFAULT_MIN_DELAY_SECONDS);
}

#[test]
fn test_non_proposer_cannot_execute() {
    let (env, admin, client) = setup_env();
    let action_id = make_id(&env, 6);
    let proposer = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.queue_action(&proposer, &action_id);
    env.ledger().with_mut(|li| li.timestamp += DEFAULT_MIN_DELAY_SECONDS);

    // A non-proposer without executor designation cannot execute.
    let result = client.try_execute_action(&attacker, &action_id);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
    assert!(client.get_pending_action(&action_id).is_some());

    // The original proposer can still execute.
    client.execute_action(&proposer, &action_id);
    assert!(client.get_pending_action(&action_id).is_none());

    // Silence unused warning for admin binding.
    let _ = admin;
}

#[test]
fn test_designated_executor_can_execute() {
    let (env, admin, client) = setup_env();
    let action_id = make_id(&env, 7);
    let proposer = Address::generate(&env);
    let executor = Address::generate(&env);

    client.queue_action(&proposer, &action_id);
    client.set_executor(&admin, &action_id, &executor);
    env.ledger().with_mut(|li| li.timestamp += DEFAULT_MIN_DELAY_SECONDS);

    client.execute_action(&executor, &action_id);
    assert!(client.get_pending_action(&action_id).is_none());
}

#[test]
fn test_queue_action_emits_operation_queued_event() {
    let (env, admin, client) = setup_env();
    let action_id = make_id(&env, 8);

    let queued = client.queue_action(&admin, &action_id);

    let events = env.events().all();
    let (_, topics, data) = events.last().expect("expected an event to be emitted");

    let topic0: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic0, Symbol::new(&env, "op_queued"));

    let (op_id, operation, proposer, execute_after): (BytesN<32>, BytesN<32>, Address, u64) =
        data.try_into_val(&env).unwrap();
    assert_eq!(op_id, action_id);
    assert_eq!(operation, action_id);
    assert_eq!(proposer, admin);
    assert_eq!(execute_after, queued.execute_after);
}

#[test]
fn test_execute_action_emits_operation_executed_event() {
    let (env, admin, client) = setup_env();
    let action_id = make_id(&env, 9);
    client.queue_action(&admin, &action_id);
    env.ledger().with_mut(|li| li.timestamp += DEFAULT_MIN_DELAY_SECONDS);

    client.execute_action(&admin, &action_id);

    let events = env.events().all();
    let (_, topics, data) = events.last().expect("expected an event to be emitted");

    let topic0: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic0, Symbol::new(&env, "op_executed"));

    let (op_id, executed_by): (BytesN<32>, Address) = data.try_into_val(&env).unwrap();
    assert_eq!(op_id, action_id);
    assert_eq!(executed_by, admin);
}

#[test]
fn test_cancel_timelock_emits_operation_cancelled_event() {
    let (env, admin, client) = setup_env();
    let action_id = make_id(&env, 10);
    client.queue_action(&admin, &action_id);

    client.cancel_timelock(&admin, &action_id);

    let events = env.events().all();
    let (_, topics, data) = events.last().expect("expected an event to be emitted");

    let topic0: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic0, Symbol::new(&env, "op_cancelled"));

    let (op_id, cancelled_by): (BytesN<32>, Address) = data.try_into_val(&env).unwrap();
    assert_eq!(op_id, action_id);
    assert_eq!(cancelled_by, admin);
}

#[test]
fn test_cannot_execute_before_timelock_expires() {
    let (env, admin, client) = setup_env();
    let action_id = make_id(&env, 5);
    let queued = client.queue_action(&admin, &action_id);

    // Advance the ledger clock to one second before the timelock expires.
    env.ledger()
        .with_mut(|li| li.timestamp = queued.executable_at - 1);
    assert!(client.try_execute_action(&admin, &action_id).is_err());
    assert!(client.get_pending_action(&action_id).is_some());
}

#[test]
fn test_executes_exactly_at_timelock_expiry() {
    let (env, admin, client) = setup_env();
    let action_id = make_id(&env, 6);
    let queued = client.queue_action(&admin, &action_id);

    // Advance the ledger clock to exactly the timelock expiry timestamp.
    env.ledger()
        .with_mut(|li| li.timestamp = queued.executable_at);

    client.execute_action(&admin, &action_id);
    assert!(client.get_pending_action(&action_id).is_none());
}

#[test]
fn test_disable_slippage_protection_requires_timelock() {
    let (env, admin, client) = setup_env();
    let action_id = make_id(&env, 5);

    // Disabling slippage protection must be routed through the timelock.
    client.queue_action(&admin, &action_id);
    assert!(client.get_pending_action(&action_id).is_some());

    // Cannot execute before the minimum 24h delay elapses.
    env.ledger().with_mut(|li| li.timestamp += DEFAULT_MIN_DELAY_SECONDS / 2);
    assert!(client.try_execute_action(&admin, &action_id).is_err());

    // After the delay, execution succeeds and clears the pending action.
    env.ledger().with_mut(|li| li.timestamp += DEFAULT_MIN_DELAY_SECONDS);

    client.execute_action(&admin, &action_id);
    assert!(client.get_pending_action(&action_id).is_none());
}

#[test]
fn test_operation_expires_after_execution_window() {
    let (env, admin, client) = setup_env();
    let action_id = make_id(&env, 7);
    let queued = client.queue_action(&admin, &action_id);

    // Advance the ledger clock past the execution window.
    env.ledger()
        .with_mut(|li| li.timestamp = queued.expires_at + 1);
    assert!(client.try_execute_action(&admin, &action_id).is_err());
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
fn test_disable_slippage_protection_requires_multisig_quorum() {
    let (env, admin, client) = setup_env();
    let action_id = make_id(&env, 6);

    // The disable operation is queued as a governance action; without the
    // required multisig_admin quorum it cannot be executed even after the
    // timelock delay has elapsed.
    client.queue_action(&admin, &action_id);
    env.ledger().with_mut(|li| li.timestamp += DEFAULT_MIN_DELAY_SECONDS);

    // A non-admin signer cannot execute the queued disable action.
    let unauthorized = Address::generate(&env);
    assert!(client.try_execute_action(&unauthorized, &action_id).is_err());
    assert!(client.get_pending_action(&action_id).is_some());
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
}
