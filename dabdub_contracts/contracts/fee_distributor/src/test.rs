#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
    vec, Address, Env,
};

use crate::{
    FeeDistributorContract, FeeDistributorContractClient, FeeRecipient, PendingRecipientUpdate,
};

struct Setup<'a> {
    env: Env,
    admin: Address,
    treasury: Address,
    lp: Address,
    caller: Address,
    contract_id: Address,
    token: TokenClient<'a>,
    client: FeeDistributorContractClient<'a>,
}

fn setup(shares: &[u32]) -> Setup<'static> {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let mut recipients = soroban_sdk::Vec::new(&env);
    for share_bps in shares {
        recipients.push_back(FeeRecipient {
            address: Address::generate(&env),
            share_bps: *share_bps,
        });
    }
    let treasury = recipients.get(0).unwrap().address;
    let lp = recipients.get(1).unwrap().address;
    let caller = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let token = TokenClient::new(&env, &token_id.address());
    StellarAssetClient::new(&env, &token_id.address()).mint(&caller, &1_000_000);

    let contract_id = env.register(
        FeeDistributorContract,
        (&admin, recipients, &token_id.address(), &caller),
    );
    let client = FeeDistributorContractClient::new(&env, &contract_id);

    Setup {
        env,
        admin,
        treasury,
        lp,
        caller,
        contract_id,
        token,
        client,
    }
}

fn default_setup() -> Setup<'static> {
    setup(&[5_000, 5_000])
}

fn schedule_update(
    setup: &Setup<'_>,
    recipients: soroban_sdk::Vec<FeeRecipient>,
) -> PendingRecipientUpdate {
    setup.client.schedule_recipients(&setup.admin, &recipients);
    setup.client.get_pending_recipients().unwrap()
}

#[test]
fn test_distributes_to_each_recipient() {
    let s = default_setup();
    s.client.distribute(&s.caller, &10_000);
    assert_eq!(s.token.balance(&s.treasury), 5_000);
    assert_eq!(s.token.balance(&s.lp), 5_000);
    assert_eq!(s.token.balance(&s.contract_id), 0);
}

#[test]
fn test_three_recipient_split_conserves_rounding_remainder() {
    let s = setup(&[3_333, 3_333, 3_334]);
    let recipients = s.client.get_recipients();
    let first = recipients.get(0).unwrap().address;
    let second = recipients.get(1).unwrap().address;
    let final_recipient = recipients.get(2).unwrap().address;

    s.client.distribute(&s.caller, &10);

    assert_eq!(s.token.balance(&first), 3);
    assert_eq!(s.token.balance(&second), 3);
    assert_eq!(s.token.balance(&final_recipient), 4);
    assert_eq!(s.token.balance(&s.contract_id), 0);
}

#[test]
fn test_single_recipient_receives_full_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let recipient = Address::generate(&env);
    let recipients = vec![
        &env,
        FeeRecipient {
            address: recipient.clone(),
            share_bps: 10_000,
        },
    ];
    // The setup's convenience fields need two recipients, so register directly.
    let admin = Address::generate(&env);
    let caller = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let token = TokenClient::new(&env, &token_id.address());
    StellarAssetClient::new(&env, &token_id.address()).mint(&caller, &100);
    let contract_id = env.register(
        FeeDistributorContract,
        (&admin, recipients, &token_id.address(), &caller),
    );
    FeeDistributorContractClient::new(&env, &contract_id).distribute(&caller, &10);
    assert_eq!(token.balance(&recipient), 10);
    assert_eq!(token.balance(&contract_id), 0);
}

#[test]
#[should_panic(expected = "recipient shares must total 10000 bps")]
fn test_constructor_rejects_invalid_share_total() {
    let env = Env::default();
    let recipients = vec![
        &env,
        FeeRecipient {
            address: Address::generate(&env),
            share_bps: 4_000,
        },
    ];
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let caller = Address::generate(&env);
    env.register(
        FeeDistributorContract,
        (&admin, recipients, &token_id.address(), &caller),
    );
}

#[test]
#[should_panic(expected = "recipient count must be between 1 and 10")]
fn test_constructor_rejects_empty_recipient_list() {
    let env = Env::default();
    let recipients: soroban_sdk::Vec<FeeRecipient> = soroban_sdk::Vec::new(&env);
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let caller = Address::generate(&env);
    env.register(
        FeeDistributorContract,
        (&admin, recipients, &token_id.address(), &caller),
    );
}

#[test]
#[should_panic(expected = "recipient addresses must be unique")]
fn test_constructor_rejects_duplicate_addresses() {
    let env = Env::default();
    let address = Address::generate(&env);
    let recipients = vec![
        &env,
        FeeRecipient {
            address: address.clone(),
            share_bps: 5_000,
        },
        FeeRecipient {
            address,
            share_bps: 5_000,
        },
    ];
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let caller = Address::generate(&env);
    env.register(
        FeeDistributorContract,
        (&admin, recipients, &token_id.address(), &caller),
    );
}

#[test]
fn test_recipient_update_waits_for_timelock() {
    let s = default_setup();
    let replacement = Address::generate(&s.env);
    let pending = schedule_update(
        &s,
        vec![
            &s.env,
            FeeRecipient {
                address: replacement.clone(),
                share_bps: 10_000,
            },
        ],
    );

    assert_eq!(
        pending.execute_after,
        s.env.ledger().sequence() + crate::RECIPIENT_UPDATE_DELAY_LEDGERS
    );
    assert!(s.client.try_apply_recipients(&s.admin).is_err());
    assert_eq!(
        s.client.get_recipients().get(0).unwrap().address,
        s.treasury
    );

    s.env.ledger().set_sequence_number(pending.execute_after);
    s.client.apply_recipients(&s.admin);
    let active = s.client.get_recipients();
    assert_eq!(active.len(), 1);
    assert_eq!(active.get(0).unwrap().address, replacement);
    assert!(s.client.get_pending_recipients().is_none());
}

#[test]
fn test_non_admin_cannot_schedule_or_apply_recipient_update() {
    let s = default_setup();
    let stranger = Address::generate(&s.env);
    let replacements = vec![
        &s.env,
        FeeRecipient {
            address: Address::generate(&s.env),
            share_bps: 10_000,
        },
    ];
    assert!(s
        .client
        .try_schedule_recipients(&stranger, &replacements)
        .is_err());
    assert!(s.client.try_apply_recipients(&stranger).is_err());
}

#[test]
fn test_admin_can_cancel_pending_update() {
    let s = default_setup();
    let replacement = Address::generate(&s.env);
    schedule_update(
        &s,
        vec![
            &s.env,
            FeeRecipient {
                address: replacement,
                share_bps: 10_000,
            },
        ],
    );

    s.client.cancel_recipients_update(&s.admin);
    assert!(s.client.get_pending_recipients().is_none());
    assert_eq!(s.client.get_recipients().len(), 2);
}

#[test]
fn test_distribution_emits_payouts_event() {
    let s = default_setup();
    let event_count = s.env.events().all().len();
    s.client.distribute(&s.caller, &10_000);
    assert!(s.env.events().all().len() > event_count);
}

#[test]
fn test_allowed_caller_rotation_and_admin_rotation() {
    let s = default_setup();
    let new_caller = Address::generate(&s.env);
    let sac = StellarAssetClient::new(&s.env, &s.token.address);
    sac.mint(&new_caller, &10_000);
    s.client.set_allowed_caller(&s.admin, &new_caller);
    assert!(s.client.try_distribute(&s.caller, &10_000).is_err());
    s.client.distribute(&new_caller, &10_000);

    let new_admin = Address::generate(&s.env);
    s.client.transfer_admin(&s.admin, &new_admin);
    let updated = vec![
        &s.env,
        FeeRecipient {
            address: Address::generate(&s.env),
            share_bps: 10_000,
        },
    ];
    assert!(s
        .client
        .try_schedule_recipients(&s.admin, &updated)
        .is_err());
    s.client.schedule_recipients(&new_admin, &updated);
}

#[test]
#[should_panic(expected = "total_amount must be > 0")]
fn test_zero_amount_rejected() {
    let s = default_setup();
    s.client.distribute(&s.caller, &0);
}
