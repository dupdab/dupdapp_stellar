//! Shared test fixtures for the dabdub contracts.
//!
//! Each contract's `src/test.rs` previously reimplemented the same helpers
//! (deploying the contract, funding a test account, creating a payment).
//! Those helpers now live here so tests stay short and consistent.

#![cfg(any(test, feature = "testutils"))]

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, IntoVal, Symbol, Val, Vec,
};

/// Default starting balance used when funding a test account.
pub const DEFAULT_FUND_AMOUNT: i128 = 1_000_000_000;

/// Register a contract from its WASM and return its address.
///
/// `wasm` is the contract's compiled WASM (e.g. `dabdub_contract::WASM`).
pub fn deploy_contract(env: &Env, wasm: &[u8]) -> Address {
    let contract_id = env.register(wasm, ());
    contract_id
}

/// Register a contract from a constructor argument tuple and return its address.
pub fn deploy_contract_with_args<A>(env: &Env, wasm: &[u8], args: A) -> Address
where
    A: IntoVal<Env, Vec<Val>>,
{
    env.register(wasm, args)
}

/// Create a fresh test account and fund it with a Stellar asset contract.
///
/// Returns the funded account address and the token contract address.
pub fn fund_account(env: &Env, amount: i128) -> (Address, Address) {
    let account = Address::generate(env);
    let admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(admin).address();
    let token_client = token::StellarAssetClient::new(env, &token_id);
    token_client.mint(&account, &amount);
    (account, token_id)
}

/// Create a fresh test account funded with [`DEFAULT_FUND_AMOUNT`].
pub fn fund_default_account(env: &Env) -> (Address, Address) {
    fund_account(env, DEFAULT_FUND_AMOUNT)
}

/// Invoke a contract function with the given arguments and return its result.
///
/// This is a thin wrapper around `env.invoke_contract` so tests can create
/// payments (and other calls) without repeating the argument plumbing.
pub fn invoke<Ret: IntoVal<Env, Val>>(
    env: &Env,
    contract_id: &Address,
    func: &str,
    args: Vec<Val>,
) -> Ret {
    env.invoke_contract(contract_id, &Symbol::new(env, func), args)
}

/// Create a payment by invoking `create_payment` on the given contract.
///
/// The exact argument shape is contract-specific, so callers pass the
/// already-built argument vector; this helper centralises the invocation
/// and returns the resulting payment id.
pub fn create_payment<Ret: IntoVal<Env, Val>>(
    env: &Env,
    contract_id: &Address,
    args: Vec<Val>,
) -> Ret {
    invoke(env, contract_id, "create_payment", args)
}

/// Advance the ledger timestamp by `seconds`.
pub fn advance_time(env: &Env, seconds: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp += seconds;
    });
}
