#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env,
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
