#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env};

const DEFAULT_MIN_DELAY_SECONDS: u64 = 48 * 60 * 60;

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TimelockAction {
    pub action_id: BytesN<32>,
    pub queued_at: u64,
    pub execute_after: u64,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    MinDelay,
    PendingAction(BytesN<32>),
}

#[contract]
pub struct AdminTimelockContract;

#[contractimpl]
impl AdminTimelockContract {
    pub fn __constructor(env: Env, admin: Address, min_delay_seconds: u64) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::MinDelay, &min_delay_seconds.max(DEFAULT_MIN_DELAY_SECONDS));
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    pub fn get_min_delay(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::MinDelay)
            .unwrap_or(DEFAULT_MIN_DELAY_SECONDS)
    }

    pub fn set_min_delay(env: Env, caller: Address, min_delay_seconds: u64) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        if min_delay_seconds == 0 {
            panic!("minimum delay must be > 0");
        }
        env.storage()
            .instance()
            .set(&DataKey::MinDelay, &min_delay_seconds);
    }

    pub fn queue_action(env: Env, caller: Address, action_id: BytesN<32>) -> TimelockAction {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        let now = env.ledger().timestamp();
        let delay = Self::get_min_delay(env.clone());
        let action = TimelockAction {
            action_id: action_id.clone(),
            queued_at: now,
            execute_after: now.saturating_add(delay),
        };
        env.storage()
            .persistent()
            .set(&DataKey::PendingAction(action_id.clone()), &action);
        action
    }

    pub fn schedule_change(env: Env, caller: Address, action_id: BytesN<32>) -> TimelockAction {
        Self::queue_action(env, caller, action_id)
    }

    pub fn cancel_timelock(env: Env, caller: Address, action_id: BytesN<32>) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        let key = DataKey::PendingAction(action_id.clone());
        if !env.storage().persistent().has(&key) {
            panic!("pending action not found");
        }
        env.storage().persistent().remove(&key);
    }

    pub fn cancel_change(env: Env, caller: Address, action_id: BytesN<32>) {
        Self::cancel_timelock(env, caller, action_id);
    }

    pub fn execute_action(env: Env, caller: Address, action_id: BytesN<32>) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        let key = DataKey::PendingAction(action_id.clone());
        let action: TimelockAction = env
            .storage()
            .persistent()
            .get(&key)
            .expect("pending action not found");
        if env.ledger().timestamp() < action.execute_after {
            panic!("timelock has not elapsed");
        }
        env.storage().persistent().remove(&key);
    }

    pub fn apply_change(env: Env, caller: Address, action_id: BytesN<32>) {
        Self::execute_action(env, caller, action_id);
    }

    pub fn get_pending_action(env: Env, action_id: BytesN<32>) -> Option<TimelockAction> {
        env.storage()
            .persistent()
            .get(&DataKey::PendingAction(action_id))
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if caller != &admin {
            panic!("Not admin");
        }
    }
}
