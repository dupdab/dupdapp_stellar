#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env};

/// Default minimum delay applied when the configured delay is below this floor.
///
/// Value: 48 hours (172_800 seconds).
const DEFAULT_MIN_DELAY_SECONDS: u64 = 48 * 60 * 60;

/// A queued timelock action awaiting execution.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TimelockAction {
    /// Unique 32-byte identifier of the action.
    pub action_id: BytesN<32>,
    /// Ledger timestamp (seconds) at which the action was queued.
    pub queued_at: u64,
    /// Ledger timestamp (seconds) at or after which the action may execute.
    pub execute_after: u64,
}

/// Storage keys used by the timelock contract.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Address of the contract admin.
    Admin,
    /// Configured minimum delay in seconds.
    MinDelay,
    /// Pending action keyed by its 32-byte action id.
    PendingAction(BytesN<32>),
}

/// Admin timelock contract enforcing a delay before privileged actions execute.
#[contract]
pub struct AdminTimelockContract;

#[contractimpl]
impl AdminTimelockContract {
    /// Initializes the contract with an admin and a minimum delay.
    ///
    /// # Parameters
    /// - `admin`: Address authorized to queue, cancel, and execute actions.
    /// - `min_delay_seconds`: Requested minimum delay in seconds. Values below
    ///   [`DEFAULT_MIN_DELAY_SECONDS`] are clamped up to that floor.
    pub fn __constructor(env: Env, admin: Address, min_delay_seconds: u64) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::MinDelay, &min_delay_seconds.max(DEFAULT_MIN_DELAY_SECONDS));
    }

    /// Returns the configured admin address.
    ///
    /// # Panics
    /// Panics if the contract has not been initialized (no admin stored).
    pub fn get_admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    /// Returns the configured minimum delay in seconds.
    ///
    /// Falls back to [`DEFAULT_MIN_DELAY_SECONDS`] if no delay is stored.
    pub fn get_min_delay(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::MinDelay)
            .unwrap_or(DEFAULT_MIN_DELAY_SECONDS)
    }

    /// Updates the minimum delay applied to newly queued actions.
    ///
    /// # Parameters
    /// - `caller`: Must be the admin; authorization is required.
    /// - `min_delay_seconds`: New delay in seconds. Must be greater than 0.
    ///
    /// # Panics
    /// - `"Not admin"` if `caller` is not the admin.
    /// - `"minimum delay must be > 0"` if `min_delay_seconds` is 0.
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

    /// Queues a new action to execute after the current minimum delay.
    ///
    /// # Parameters
    /// - `caller`: Must be the admin; authorization is required.
    /// - `action_id`: Unique 32-byte identifier for the action.
    ///
    /// # Returns
    /// The created [`TimelockAction`] with `execute_after` set to
    /// `now + min_delay` (saturating).
    ///
    /// # Panics
    /// - `"Not admin"` if `caller` is not the admin.
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

    /// Alias for [`Self::queue_action`]; schedules a change for later execution.
    ///
    /// # Parameters
    /// - `caller`: Must be the admin; authorization is required.
    /// - `action_id`: Unique 32-byte identifier for the action.
    ///
    /// # Panics
    /// - `"Not admin"` if `caller` is not the admin.
    pub fn schedule_change(env: Env, caller: Address, action_id: BytesN<32>) -> TimelockAction {
        Self::queue_action(env, caller, action_id)
    }

    /// Cancels a pending action, removing it from storage.
    ///
    /// # Parameters
    /// - `caller`: Must be the admin; authorization is required.
    /// - `action_id`: Identifier of the pending action to cancel.
    ///
    /// # Panics
    /// - `"Not admin"` if `caller` is not the admin.
    /// - `"pending action not found"` if no action exists for `action_id`.
    pub fn cancel_timelock(env: Env, caller: Address, action_id: BytesN<32>) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        let key = DataKey::PendingAction(action_id.clone());
        if !env.storage().persistent().has(&key) {
            panic!("pending action not found");
        }
        env.storage().persistent().remove(&key);
    }

    /// Alias for [`Self::cancel_timelock`]; cancels a scheduled change.
    ///
    /// # Parameters
    /// - `caller`: Must be the admin; authorization is required.
    /// - `action_id`: Identifier of the pending action to cancel.
    ///
    /// # Panics
    /// - `"Not admin"` if `caller` is not the admin.
    /// - `"pending action not found"` if no action exists for `action_id`.
    pub fn cancel_change(env: Env, caller: Address, action_id: BytesN<32>) {
        Self::cancel_timelock(env, caller, action_id);
    }

    /// Executes a pending action once its timelock has elapsed.
    ///
    /// # Parameters
    /// - `caller`: Must be the admin; authorization is required.
    /// - `action_id`: Identifier of the pending action to execute.
    ///
    /// # Panics
    /// - `"Not admin"` if `caller` is not the admin.
    /// - `"pending action not found"` if no action exists for `action_id`.
    /// - `"timelock has not elapsed"` if the current ledger timestamp is
    ///   before the action's `execute_after`.
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

    /// Alias for [`Self::execute_action`]; applies a scheduled change.
    ///
    /// # Parameters
    /// - `caller`: Must be the admin; authorization is required.
    /// - `action_id`: Identifier of the pending action to apply.
    ///
    /// # Panics
    /// - `"Not admin"` if `caller` is not the admin.
    /// - `"pending action not found"` if no action exists for `action_id`.
    /// - `"timelock has not elapsed"` if the timelock has not yet elapsed.
    pub fn apply_change(env: Env, caller: Address, action_id: BytesN<32>) {
        Self::execute_action(env, caller, action_id);
    }

    /// Returns the pending action for `action_id`, if any.
    ///
    /// # Parameters
    /// - `action_id`: Identifier of the action to look up.
    pub fn get_pending_action(env: Env, action_id: BytesN<32>) -> Option<TimelockAction> {
        env.storage()
            .persistent()
            .get(&DataKey::PendingAction(action_id))
    }

    /// Ensures `caller` matches the stored admin.
    ///
    /// # Panics
    /// - `"Not admin"` if `caller` is not the admin.
    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if caller != &admin {
            panic!("Not admin");
        }
    }
}
