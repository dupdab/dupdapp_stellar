#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events as _, MockAuth, MockAuthInvoke},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env, IntoVal, Symbol,
};

use crate::{FeeDistributorContract, FeeDistributorContractClient};

struct Setup<'a> {
    env: Env,
    admin: Address,
    treasury: Address,
    lp: Address,
    caller: Address,
    token: TokenClient<'a>,
    client: FeeDistributorContractClient<'a>,
}

fn setup(lp_share_bps: i128) -> Setup<'static> {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let lp = Address::generate(&env);
    let caller = Address::generate(&env);

    // Deploy a test token (Stellar Asset Contract)
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let sac = StellarAssetClient::new(&env, &token_id.address());
    // Mint 1_000_000 to caller so distribute() can pull from them
    sac.mint(&caller, &1_000_000);

    let contract_id = env.register(
        FeeDistributorContract,
        (&admin, &treasury, &lp, lp_share_bps, &token_id.address(), &caller),
    );

    let client = FeeDistributorContractClient::new(&env, &contract_id);
    let token = TokenClient::new(&env, &token_id.address());

    Setup { env, admin, treasury, lp, caller, token, client }
}

#[test]
fn test_50_50_split() {
    let s = setup(5_000); // 50% LP
    s.client.distribute(&s.caller, &10_000);
    assert_eq!(s.token.balance(&s.treasury), 5_000);
    assert_eq!(s.token.balance(&s.lp), 5_000);
}

#[test]
fn test_30_70_split() {
    let s = setup(3_000); // 30% LP, 70% treasury
    s.client.distribute(&s.caller, &10_000);
    assert_eq!(s.token.balance(&s.lp), 3_000);
    assert_eq!(s.token.balance(&s.treasury), 7_000);
}

#[test]
fn test_zero_lp_share() {
    let s = setup(0); // 0% LP — all goes to treasury
    s.client.distribute(&s.caller, &10_000);
    assert_eq!(s.token.balance(&s.treasury), 10_000);
    assert_eq!(s.token.balance(&s.lp), 0);
}

#[test]
fn test_hundred_percent_lp() {
    let s = setup(10_000); // 100% LP — nothing to treasury
    s.client.distribute(&s.caller, &10_000);
    assert_eq!(s.token.balance(&s.lp), 10_000);
    assert_eq!(s.token.balance(&s.treasury), 0);
}

#[test]
fn test_rounding_remainder_goes_to_treasury() {
    let s = setup(3_333); // 33.33% LP
    s.client.distribute(&s.caller, &10);
    // lp = 10 * 3333 / 10000 = 3 (truncated), treasury = 7
    assert_eq!(s.token.balance(&s.lp), 3);
    assert_eq!(s.token.balance(&s.treasury), 7);
}

#[test]
fn test_update_lp_share() {
    let s = setup(5_000);
    s.client.set_lp_share(&s.admin, &2_000);
    let (_, _, bps) = s.client.get_config();
    assert_eq!(bps, 2_000);
    s.client.distribute(&s.caller, &10_000);
    assert_eq!(s.token.balance(&s.lp), 2_000);
    assert_eq!(s.token.balance(&s.treasury), 8_000);
}

#[test]
fn test_update_addresses() {
    let s = setup(5_000);
    let new_treasury = Address::generate(&s.env);
    let new_lp = Address::generate(&s.env);
    s.client.set_treasury(&s.admin, &new_treasury);
    s.client.set_lp_address(&s.admin, &new_lp);
    s.client.distribute(&s.caller, &10_000);
    assert_eq!(s.token.balance(&new_treasury), 5_000);
    assert_eq!(s.token.balance(&new_lp), 5_000);
    // old addresses untouched
    assert_eq!(s.token.balance(&s.treasury), 0);
    assert_eq!(s.token.balance(&s.lp), 0);
}

/// `distribute` intentionally has NO caller allowlist: any address holding the
/// token may call it and have its own funds split between treasury and LP.
/// This test mints tokens to a second, unrelated address (distinct from the
/// pre-minted `caller` in `setup()`) and calls `distribute` from it, asserting
/// the current unrestricted behavior explicitly. If a future change adds a
/// caller restriction, this test must be updated deliberately — making any
/// access-control change a conscious, test-visible decision.
#[test]
fn test_distribute_from_unrelated_caller_is_unrestricted() {
    let s = setup(5_000); // 50% LP

    // A second, unrelated address that is not the pre-minted `caller`.
    let unrelated = Address::generate(&s.env);
    assert_ne!(unrelated, s.caller);

    // Fund the unrelated caller so distribute() can pull from them.
    let sac = StellarAssetClient::new(&s.env, &s.token.address);
    sac.mint(&unrelated, &10_000);

    // No caller allowlist: an unrelated caller can distribute their own funds.
    s.client.distribute(&unrelated, &10_000);

    assert_eq!(s.token.balance(&s.lp), 5_000);
    assert_eq!(s.token.balance(&s.treasury), 5_000);
    // The unrelated caller's funds were consumed by the distribution.
    assert_eq!(s.token.balance(&unrelated), 0);
    // The original pre-minted caller was not touched.
    assert_eq!(s.token.balance(&s.caller), 1_000_000);
}

#[test]
#[should_panic(expected = "treasury and lp_address must differ")]
fn test_constructor_rejects_treasury_equal_lp() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let shared = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());

    env.register(
        FeeDistributorContract,
        (&admin, &shared, &shared, 5_000i128, &token_id.address()),
    );
}

#[test]
#[should_panic(expected = "admin must differ from treasury")]
fn test_constructor_rejects_admin_equal_treasury() {
    let env = Env::default();
    env.mock_all_auths();

    let shared = Address::generate(&env);
    let lp = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());

    env.register(
        FeeDistributorContract,
        (&shared, &shared, &lp, 5_000i128, &token_id.address()),
    );
}

#[test]
#[should_panic(expected = "admin must differ from lp_address")]
fn test_constructor_rejects_admin_equal_lp() {
    let env = Env::default();
    env.mock_all_auths();

    let treasury = Address::generate(&env);
    let shared = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());

    env.register(
        FeeDistributorContract,
        (&shared, &treasury, &shared, 5_000i128, &token_id.address()),
    );
}

#[test]
fn test_set_lp_share_emits_event() {
    let s = setup(5_000);
    s.client.set_lp_share(&s.admin, &2_000);
    let events = s.env.events().all();
    let last = events.last().unwrap();
    assert_eq!(last.1, (Symbol::new(&s.env, "lp_share_set"),).into_val(&s.env));
    let (old, new): (i128, i128) = last.2.into_val(&s.env);
    assert_eq!(old, 5_000);
    assert_eq!(new, 2_000);
}

#[test]
fn test_set_treasury_emits_event() {
    let s = setup(5_000);
    let new_treasury = Address::generate(&s.env);
    s.client.set_treasury(&s.admin, &new_treasury);
    let events = s.env.events().all();
    let last = events.last().unwrap();
    assert_eq!(last.1, (Symbol::new(&s.env, "treasury_set"),).into_val(&s.env));
    let (old, new): (Address, Address) = last.2.into_val(&s.env);
    assert_eq!(old, s.treasury);
    assert_eq!(new, new_treasury);
}

#[test]
fn test_set_lp_address_emits_event() {
    let s = setup(5_000);
    let new_lp = Address::generate(&s.env);
    s.client.set_lp_address(&s.admin, &new_lp);
    let events = s.env.events().all();
    let last = events.last().unwrap();
    assert_eq!(last.1, (Symbol::new(&s.env, "lp_address_set"),).into_val(&s.env));
    let (old, new): (Address, Address) = last.2.into_val(&s.env);
    assert_eq!(old, s.lp);
    assert_eq!(new, new_lp);
}

#[test]
fn test_unauthorized_caller_rejected() {
    let s = setup(5_000);
    let stranger = Address::generate(&s.env);
    let sac = StellarAssetClient::new(&s.env, &s.token.address);
    sac.mint(&stranger, &10_000);
    let res = s.client.try_distribute(&stranger, &10_000);
    assert!(res.is_err());
    assert_eq!(s.token.balance(&s.treasury), 0);
    assert_eq!(s.token.balance(&s.lp), 0);
}

#[test]
fn test_rotate_allowed_caller() {
    let s = setup(5_000);
    let new_caller = Address::generate(&s.env);
    let sac = StellarAssetClient::new(&s.env, &s.token.address);
    sac.mint(&new_caller, &10_000);

    // Old caller works before rotation
    s.client.distribute(&s.caller, &10_000);
    assert_eq!(s.token.balance(&s.treasury), 5_000);

    // Rotate to new caller
    s.client.set_allowed_caller(&s.admin, &new_caller);

    // Old caller now rejected
    let res = s.client.try_distribute(&s.caller, &10_000);
    assert!(res.is_err());

    // New caller works
    s.client.distribute(&new_caller, &10_000);
    assert_eq!(s.token.balance(&s.treasury), 10_000);
    assert_eq!(s.token.balance(&s.lp), 10_000);
}

#[test]
fn test_transfer_admin_rotates_admin() {
    let s = setup(5_000);
    let new_admin = Address::generate(&s.env);

    // Rotate admin
    s.client.transfer_admin(&s.admin, &new_admin);

    // Old admin can no longer perform gated calls
    let res = s.client.try_set_lp_share(&s.admin, &2_000);
    assert!(res.is_err());

    // New admin can perform gated calls
    s.client.set_lp_share(&new_admin, &2_000);
    let (_, _, bps) = s.client.get_config();
    assert_eq!(bps, 2_000);
}

#[test]
fn test_transfer_admin_rejects_non_admin() {
    let s = setup(5_000);
    let stranger = Address::generate(&s.env);
    let new_admin = Address::generate(&s.env);

    let res = s.client.try_transfer_admin(&stranger, &new_admin);
    assert!(res.is_err());

    // Admin unchanged: original admin still works
    s.client.set_lp_share(&s.admin, &2_000);
    let (_, _, bps) = s.client.get_config();
    assert_eq!(bps, 2_000);
}
}
