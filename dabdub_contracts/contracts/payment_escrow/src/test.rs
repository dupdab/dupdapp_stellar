#![cfg(test)]

use crate::{AssetType, PaymentEscrowContract, PaymentEscrowContractClient, PaymentStatus};
use merchant_registry::{MerchantRegistryContract, MerchantRegistryContractClient};
use soroban_sdk::{
    testutils::Address as _, testutils::Ledger, token, Address, BytesN, Env, String,
};

const DEFAULT_PAYMENT_TTL: u32 = 100;
const EMERGENCY_COOLDOWN_LEDGERS: u32 = 50;

type Setup = (
    Env,
    PaymentEscrowContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
);

fn deploy_escrow(
    env: &Env,
    admin: &Address,
    xlm: &Address,
    usdc: &Address,
    registry: Option<Address>,
    emergency_signers: &soroban_sdk::Vec<Address>,
    emergency_treasury: &Address,
) -> Address {
    env.register(
        PaymentEscrowContract,
        (
            admin,
            xlm,
            usdc,
            &DEFAULT_PAYMENT_TTL,
            &registry,
            emergency_signers,
            emergency_treasury,
            &EMERGENCY_COOLDOWN_LEDGERS,
        ),
    )
}

fn setup_env() -> Setup {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_sequence_number(10);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let usdc = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let xlm = env.register_stellar_asset_contract_v2(token_admin).address();

    let emergency_signer_one = Address::generate(&env);
    let emergency_signer_two = Address::generate(&env);
    let emergency_signer_three = Address::generate(&env);
    let emergency_treasury = Address::generate(&env);
    let emergency_signers = soroban_sdk::vec![
        &env,
        emergency_signer_one.clone(),
        emergency_signer_two.clone(),
        emergency_signer_three.clone(),
    ];

    let contract_id = deploy_escrow(
        &env,
        &admin,
        &xlm,
        &usdc,
        None,
        &emergency_signers,
        &emergency_treasury,
    );
    let client = PaymentEscrowContractClient::new(&env, &contract_id);

    token::StellarAssetClient::new(&env, &usdc).mint(&customer, &1_000_000_000i128);

    (
        env,
        client,
        contract_id,
        admin,
        customer,
        merchant,
        usdc,
        xlm,
        emergency_signer_one,
        emergency_signer_two,
        emergency_signer_three,
        emergency_treasury,
    )
}

fn setup_with_registry() -> (
    Env,
    PaymentEscrowContractClient<'static>,
    MerchantRegistryContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_sequence_number(10);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let usdc = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let xlm = env.register_stellar_asset_contract_v2(token_admin).address();

    let emergency_signer_one = Address::generate(&env);
    let emergency_signer_two = Address::generate(&env);
    let emergency_signer_three = Address::generate(&env);
    let emergency_treasury = Address::generate(&env);
    let emergency_signers = soroban_sdk::vec![
        &env,
        emergency_signer_one.clone(),
        emergency_signer_two.clone(),
        emergency_signer_three.clone(),
    ];

    let registry_id = env.register(MerchantRegistryContract, (&admin,));
    let registry = MerchantRegistryContractClient::new(&env, &registry_id);

    let escrow_id = deploy_escrow(
        &env,
        &admin,
        &xlm,
        &usdc,
        Some(registry_id.clone()),
        &emergency_signers,
        &emergency_treasury,
    );
    let escrow = PaymentEscrowContractClient::new(&env, &escrow_id);

    token::StellarAssetClient::new(&env, &usdc).mint(&customer, &1_000_000_000i128);

    (
        env,
        escrow,
        registry,
        admin,
        customer,
        merchant,
        usdc,
        emergency_signer_one,
        emergency_signer_two,
        emergency_signer_three,
        emergency_treasury,
    )
}

fn make_id(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn deposit_default_ttl(
    client: &PaymentEscrowContractClient<'_>,
    customer: &Address,
    payment_id: &BytesN<32>,
    merchant: &Address,
    amount: i128,
) {
    client.deposit(
        customer,
        payment_id,
        merchant,
        &amount,
        &DEFAULT_PAYMENT_TTL,
        &AssetType::Usdc,
    );
}

#[test]
fn test_constructor() {
    let (env, client, _contract_id, admin, _customer, _merchant, usdc, xlm, ..) = setup_env();

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_usdc_token(), usdc);
    assert_eq!(client.get_xlm_token(), xlm);
    assert_eq!(client.get_default_ttl_ledgers(), DEFAULT_PAYMENT_TTL);
    assert_eq!(client.get_version(), 1);
    assert_eq!(client.get_registry(), None);
}

#[test]
fn test_deposit_happy_path() {
    let (env, client, contract_id, _admin, customer, merchant, usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    client.deposit(
        &customer,
        &payment_id,
        &merchant,
        &250_000_000i128,
        &DEFAULT_PAYMENT_TTL,
        &AssetType::Usdc,
    );

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.amount, 250_000_000);
    assert_eq!(payment.released_amount, 0);
    assert_eq!(payment.customer, customer);
    assert_eq!(payment.merchant, merchant);
    assert_eq!(payment.status, PaymentStatus::Pending);
    assert_eq!(payment.expiry, 110);
    assert_eq!(payment.dispute_window_end, 110);
    assert_eq!(payment.dispute_reason, None);
    assert_eq!(payment.asset_type, AssetType::Usdc);
    assert_eq!(client.get_balance(&payment_id), 250_000_000);
    assert_eq!(client.get_expiry(&payment_id), 110);

    let token_client = token::Client::new(&env, &usdc);
    assert_eq!(token_client.balance(&customer), 750_000_000);
    assert_eq!(token_client.balance(&contract_id), 250_000_000);
}

#[test]
#[should_panic(expected = "Payment ID already exists")]
fn test_deposit_duplicate_payment_id() {
    let (env, client, _contract_id, _admin, customer, merchant, _usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
}

#[test]
#[should_panic(expected = "Amount must be > 0")]
fn test_deposit_zero_amount() {
    let (env, client, _contract_id, _admin, customer, merchant, _usdc, ..) = setup_env();
    client.deposit(
        &customer,
        &make_id(&env, 1),
        &merchant,
        &0i128,
        &DEFAULT_PAYMENT_TTL,
        &AssetType::Usdc,
    );
}

#[test]
#[should_panic(expected = "TTL must be > 0")]
fn test_deposit_zero_ttl() {
    let (env, client, _contract_id, _admin, customer, merchant, _usdc, ..) = setup_env();
    client.deposit(
        &customer,
        &make_id(&env, 1),
        &merchant,
        &250_000_000i128,
        &0u32,
        &AssetType::Usdc,
    );
}

#[test]
#[should_panic(expected = "TTL exceeds maximum")]
fn test_deposit_excessive_ttl() {
    let (env, client, _contract_id, _admin, customer, merchant, _usdc, ..) = setup_env();
    let ttl = client.get_max_ttl_ledgers() + 1;
    client.deposit(
        &customer,
        &make_id(&env, 1),
        &merchant,
        &250_000_000i128,
        &ttl,
        &AssetType::Usdc,
    );
}

#[test]
fn test_release_happy_path() {
    let (env, client, contract_id, admin, customer, merchant, usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    client.release(&admin, &payment_id);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Released);
    assert_eq!(payment.released_amount, 250_000_000);
    assert_eq!(client.get_balance(&payment_id), 0);

    let token_client = token::Client::new(&env, &usdc);
    assert_eq!(token_client.balance(&merchant), 250_000_000);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn test_partial_release_multiple_steps() {
    let (env, client, contract_id, admin, customer, merchant, usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    client.release_partial(&admin, &payment_id, &100_000_000i128);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Pending);
    assert_eq!(payment.released_amount, 100_000_000);
    assert_eq!(client.get_balance(&payment_id), 150_000_000);

    client.release_partial(&admin, &payment_id, &150_000_000i128);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Released);
    assert_eq!(payment.released_amount, 250_000_000);

    let token_client = token::Client::new(&env, &usdc);
    assert_eq!(token_client.balance(&merchant), 250_000_000);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
#[should_panic(expected = "Release amount exceeds remaining balance")]
fn test_partial_release_prevents_over_release() {
    let (env, client, _contract_id, admin, customer, merchant, _usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    client.release_partial(&admin, &payment_id, &200_000_000i128);
    client.release_partial(&admin, &payment_id, &100_000_000i128);
}

#[test]
#[should_panic(expected = "Release amount must be > 0")]
fn test_partial_release_zero_amount() {
    let (env, client, _contract_id, admin, customer, merchant, _usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    client.release_partial(&admin, &payment_id, &0i128);
}

#[test]
#[should_panic(expected = "Payment fully released")]
fn test_partial_release_fully_released_payment() {
    let (env, client, _contract_id, admin, customer, merchant, _usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    client.release(&admin, &payment_id);
    client.release_partial(&admin, &payment_id, &1i128);
}

#[test]
fn test_refund_returns_remaining_balance_after_partial_release() {
    let (env, client, contract_id, admin, customer, merchant, usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    client.release_partial(&admin, &payment_id, &100_000_000i128);
    env.ledger().set_sequence_number(111);
    client.refund(&payment_id);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Expired);
    assert_eq!(client.get_balance(&payment_id), 0);

    let token_client = token::Client::new(&env, &usdc);
    assert_eq!(token_client.balance(&merchant), 100_000_000);
    assert_eq!(token_client.balance(&customer), 900_000_000);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn test_expire_refunds_customer() {
    let (env, client, contract_id, _admin, customer, merchant, usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    env.ledger().set_sequence_number(111);
    client.expire(&payment_id);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Expired);
    assert_eq!(client.get_balance(&payment_id), 0);

    let token_client = token::Client::new(&env, &usdc);
    assert_eq!(token_client.balance(&customer), 1_000_000_000);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
#[should_panic(expected = "Payment has not expired")]
fn test_expire_before_ttl() {
    let (env, client, _contract_id, _admin, customer, merchant, _usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    client.expire(&payment_id);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_release_unauthorized() {
    let (env, client, _contract_id, _admin, customer, merchant, _usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);
    let random = Address::generate(&env);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    client.release(&random, &payment_id);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_release_invalid_payment_id() {
    let (env, client, _contract_id, admin, _customer, _merchant, _usdc, ..) = setup_env();
    client.release(&admin, &make_id(&env, 99));
}

#[test]
#[should_panic(expected = "Payment expired")]
fn test_release_expired_payment() {
    let (env, client, _contract_id, admin, customer, merchant, _usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    env.ledger().set_sequence_number(111);
    client.release(&admin, &payment_id);
}

#[test]
#[should_panic(expected = "Payment fully released")]
fn test_release_already_settled_payment() {
    let (env, client, _contract_id, admin, customer, merchant, _usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    client.release(&admin, &payment_id);
    client.release(&admin, &payment_id);
}

#[test]
fn test_dispute_by_customer() {
    let (env, client, _contract_id, _admin, customer, merchant, _usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    client.dispute(
        &customer,
        &payment_id,
        &String::from_str(&env, "service not delivered"),
    );

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Disputed);
    assert_eq!(
        payment.dispute_reason,
        Some(String::from_str(&env, "service not delivered"))
    );
}

#[test]
#[should_panic(expected = "Not payment participant")]
fn test_dispute_unauthorized_caller() {
    let (env, client, _contract_id, _admin, customer, merchant, _usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);
    let random = Address::generate(&env);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    client.dispute(&random, &payment_id, &String::from_str(&env, "not allowed"));
}

#[test]
#[should_panic(expected = "Dispute already open")]
fn test_duplicate_dispute_rejected() {
    let (env, client, _contract_id, _admin, customer, merchant, _usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    client.dispute(&customer, &payment_id, &String::from_str(&env, "first"));
    client.dispute(&merchant, &payment_id, &String::from_str(&env, "second"));
}

#[test]
#[should_panic(expected = "Dispute window expired")]
fn test_dispute_after_window_rejected() {
    let (env, client, _contract_id, _admin, customer, merchant, _usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    env.ledger().set_sequence_number(111);
    client.dispute(&customer, &payment_id, &String::from_str(&env, "too late"));
}

#[test]
#[should_panic(expected = "Dispute is open")]
fn test_release_blocked_while_disputed() {
    let (env, client, _contract_id, admin, customer, merchant, _usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    client.dispute(&customer, &payment_id, &String::from_str(&env, "hold funds"));
    client.release(&admin, &payment_id);
}

#[test]
fn test_resolve_dispute_to_customer() {
    let (env, client, contract_id, admin, customer, merchant, usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    client.dispute(&merchant, &payment_id, &String::from_str(&env, "chargeback"));
    client.resolve_dispute(&admin, &payment_id, &customer);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Expired);
    assert_eq!(payment.released_amount, 250_000_000);

    let token_client = token::Client::new(&env, &usdc);
    assert_eq!(token_client.balance(&customer), 1_000_000_000);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn test_resolve_dispute_to_merchant() {
    let (env, client, contract_id, admin, customer, merchant, usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    client.dispute(&customer, &payment_id, &String::from_str(&env, "investigate"));
    client.resolve_dispute(&admin, &payment_id, &merchant);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Released);
    assert_eq!(payment.released_amount, 250_000_000);

    let token_client = token::Client::new(&env, &usdc);
    assert_eq!(token_client.balance(&merchant), 250_000_000);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_resolve_dispute_unauthorized() {
    let (env, client, _contract_id, _admin, customer, merchant, _usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);
    let random = Address::generate(&env);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    client.dispute(&customer, &payment_id, &String::from_str(&env, "investigate"));
    client.resolve_dispute(&random, &payment_id, &merchant);
}

#[test]
#[should_panic(expected = "Invalid dispute winner")]
fn test_resolve_dispute_invalid_winner() {
    let (env, client, _contract_id, admin, customer, merchant, _usdc, ..) = setup_env();
    let payment_id = make_id(&env, 1);
    let random = Address::generate(&env);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    client.dispute(&merchant, &payment_id, &String::from_str(&env, "investigate"));
    client.resolve_dispute(&admin, &payment_id, &random);
}

#[test]
#[should_panic(expected = "Not admin")]
fn test_set_registry_unauthorized() {
    let (env, client, _contract_id, _admin, _customer, _merchant, _usdc, ..) = setup_env();
    let random = Address::generate(&env);
    let registry = Address::generate(&env);

    client.set_registry(&random, &Some(registry), &None);
}

// ---------------------------------------------------------------------------
// Merchant registry integration: deposit gating (#1016)
//                                                                          _
// Deploys the real merchant_registry contract as payment_escrow's registry.
// Deposit must be approved only when the merchant is Active AND KYC verified.
// ---------------------------------------------------------------------------

#[test]
fn test_deposit_allowed_when_registry_approved() {
    let (env, escrow, registry, admin, customer, merchant, _usdc, ..) = setup_with_registry();
    registry.set_kyc_status(&admin, &merchant, &true);

    let payment_id = make_id(&env, 42);
    escrow.deposit(
        &customer,
        &payment_id,
        &merchant,
        &100_000_000i128,
        &DEFAULT_PAYMENT_TTL,
        &AssetType::Usdc,
    );

    let payment = escrow.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Pending);
    assert_eq!(payment.amount, 100_000_000);
}

#[test]
#[should_panic(expected = "Merchant is not approved")]
fn test_deposit_blocked_when_kyc_not_verified() {
    let (env, escrow, _registry, _admin, customer, merchant, _usdc, ..) = setup_with_registry();
    let payment_id = make_id(&env, 43);

    escrow.deposit(
        &customer,
        &payment_id,
        &merchant,
        &100_000_000i128,
        &DEFAULT_PAYMENT_TTL,
        &AssetType::Usdc,
    );
}

#[test]
#[should_panic(expected = "Merchant is not approved")]
fn test_deposit_blocked_when_merchant_suspended() {
    let (env, escrow, registry, admin, customer, merchant, _usdc, ..) = setup_with_registry();
    registry.set_kyc_status(&admin, &merchant, &true);
    registry.suspend_merchant(&admin, &merchant);

    let payment_id = make_id(&env, 44);
    escrow.deposit(
        &customer,
        &payment_id,
        &merchant,
        &100_000_000i128,
        &DEFAULT_PAYMENT_TTL,
        &AssetType::Usdc,
    );
}

#[test]
#[should_panic(expected = "Merchant is not approved")]
fn test_deposit_blocked_for_unregistered_merchant() {
    let (env, escrow, _registry, _admin, customer, _merchant, _usdc, ..) = setup_with_registry();
    let unknown_merchant = Address::generate(&env);

    let payment_id = make_id(&env, 45);
    escrow.deposit(
        &customer,
        &payment_id,
        &unknown_merchant,
        &100_000_000i128,
        &DEFAULT_PAYMENT_TTL,
        &AssetType::Usdc,
    );
}

#[test]
fn test_emergency_drain_moves_all_funds_to_treasury() {
    let (
        env,
        client,
        contract_id,
        _admin,
        customer,
        merchant,
        usdc,
        _xlm,
        emergency_signer_one,
        emergency_signer_two,
        _emergency_signer_three,
        emergency_treasury,
    ) = setup_env();

    let payment_id = make_id(&env, 51);
    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);

    let drained = client.emergency_drain(
        &emergency_signer_one,
        &emergency_signer_one,
        &emergency_signer_two,
    );
    assert_eq!(drained, 250_000_000);

    let token_client = token::Client::new(&env, &usdc);
    assert_eq!(token_client.balance(&contract_id), 0);
    assert_eq!(token_client.balance(&emergency_treasury), 250_000_000);
}

#[test]
#[should_panic(expected = "Not emergency signer")]
fn test_emergency_drain_rejects_non_multisig_signer() {
    let (
        env,
        client,
        _contract_id,
        _admin,
        customer,
        merchant,
        _usdc,
        _xlm,
        emergency_signer_one,
        _emergency_signer_two,
        _emergency_signer_three,
        _emergency_treasury,
    ) = setup_env();

    let payment_id = make_id(&env, 52);
    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);
    let random = Address::generate(&env);

    client.emergency_drain(&emergency_signer_one, &emergency_signer_one, &random);
}

#[test]
#[should_panic(expected = "Emergency drain cooldown active")]
fn test_emergency_drain_cooldown_prevents_repeated_drain() {
    let (
        env,
        client,
        _contract_id,
        _admin,
        customer,
        merchant,
        _usdc,
        _xlm,
        emergency_signer_one,
        emergency_signer_two,
        _emergency_signer_three,
        _emergency_treasury,
    ) = setup_env();

    let payment_id_a = make_id(&env, 53);
    deposit_default_ttl(&client, &customer, &payment_id_a, &merchant, 100_000_000i128);
    client.emergency_drain(
        &emergency_signer_one,
        &emergency_signer_one,
        &emergency_signer_two,
    );

    let payment_id_b = make_id(&env, 54);
    deposit_default_ttl(&client, &customer, &payment_id_b, &merchant, 100_000_000i128);
    client.emergency_drain(
        &emergency_signer_one,
        &emergency_signer_one,
        &emergency_signer_two,
    );
}

#[test]
fn test_dispute_after_partial_release_resolves_to_customer() {
    let (env, client, contract_id, admin, customer, merchant, usdc, ..) = setup_env();
    let payment_id = make_id(&env, 55);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);

    client.release_partial(&admin, &payment_id, &100_000_000i128);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Pending);
    assert_eq!(payment.released_amount, 100_000_000);
    assert_eq!(client.get_balance(&payment_id), 150_000_000);

    client.dispute(
        &customer,
        &payment_id,
        &String::from_str(&env, "unsatisfactory delivery"),
    );

    client.resolve_dispute(&admin, &payment_id, &customer);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Expired);
    assert_eq!(payment.released_amount, 250_000_000);

    let token_client = token::Client::new(&env, &usdc);
    assert_eq!(token_client.balance(&customer), 1_000_000_000);
    assert_eq!(token_client.balance(&merchant), 100_000_000);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn test_dispute_after_partial_release_resolves_to_merchant() {
    let (env, client, contract_id, admin, customer, merchant, usdc, ..) = setup_env();
    let payment_id = make_id(&env, 56);

    deposit_default_ttl(&client, &customer, &payment_id, &merchant, 250_000_000i128);

    client.release_partial(&admin, &payment_id, &100_000_000i128);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Pending);
    assert_eq!(payment.released_amount, 100_000_000);
    assert_eq!(client.get_balance(&payment_id), 150_000_000);

    client.dispute(
        &merchant,
        &payment_id,
        &String::from_str(&env, "customer objection to chargeback"),
    );

    client.resolve_dispute(&admin, &payment_id, &merchant);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Released);
    assert_eq!(payment.released_amount, 250_000_000);

    let token_client = token::Client::new(&env, &usdc);
    assert_eq!(token_client.balance(&customer), 750_000_000);
    assert_eq!(token_client.balance(&merchant), 250_000_000);
    assert_eq!(token_client.balance(&contract_id), 0);
}
