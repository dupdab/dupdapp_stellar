#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, BytesN, Env, IntoVal, Symbol, TryIntoVal,
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
