#![no_std]

mod test;

use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, token, Address, BytesN, Env, String,
    Vec,
};

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum AssetType {
    Xlm,
    Usdc,
}

/// Thin client interface for the MerchantRegistry contract.
#[contractclient(name = "MerchantRegistryClient")]
#[allow(dead_code)]
trait MerchantRegistry {
    fn is_approved(env: Env, merchant: Address) -> bool;
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum PaymentStatus {
    Pending,
    Disputed,
    Released,
    Expired,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct PaymentEscrow {
    pub payment_id: BytesN<32>,
    pub amount: i128,
    pub released_amount: i128,
    pub merchant: Address,
    pub customer: Address,
    pub status: PaymentStatus,
    pub expiry: u32,
    pub dispute_window_end: u32,
    pub dispute_reason: Option<String>,
    pub asset_type: AssetType,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    XlmToken,
    UsdcToken,
    DefaultTtlLedgers,
    RegistryContract,
    EmergencySigners,
    EmergencyTreasury,
    EmergencyCooldownLedgers,
    EmergencyLastDrainLedger,
    UpgradeDelayLedgers,
    PendingUpgradeHash,
    PendingUpgradeLedger,
    Payment(BytesN<32>),
    Version,
}

#[contracttype]
struct DepositEvent {
    payment_id: BytesN<32>,
    customer: Address,
    merchant: Address,
    amount: i128,
    expiry: u32,
}

#[contracttype]
struct EscrowStateEvent {
    payment_id: BytesN<32>,
    amount: i128,
}

#[contracttype]
struct EmergencyDrainEvent {
    amount: i128,
    caller: Address,
}

#[contracttype]
struct UpgradeScheduledEvent {
    new_wasm_hash: BytesN<32>,
    apply_ledger: u32,
}

#[contracttype]
struct UpgradeCancelledEvent {
    new_wasm_hash: BytesN<32>,
}

const MAX_DISPUTE_WINDOW_LEDGERS: u32 = 51_840;
const MAX_TTL_LEDGERS: u32 = 518_400;
const EMERGENCY_SIGNER_COUNT: u32 = 3;
const DEFAULT_UPGRADE_DELAY_LEDGERS: u32 = 17_280;

#[contract]
pub struct PaymentEscrowContract;

#[contractimpl]
impl PaymentEscrowContract {
    pub fn __constructor(
        env: Env,
        admin: Address,
        xlm_token: Address,
        usdc_token: Address,
        default_ttl_ledgers: u32,
        registry: Option<Address>,
        emergency_signers: Vec<Address>,
        emergency_treasury: Address,
        emergency_cooldown_ledgers: u32,
    ) {
        if default_ttl_ledgers == 0 {
            panic!("Default TTL must be > 0");
        }
        if emergency_cooldown_ledgers == 0 {
            panic!("Emergency cooldown must be > 0");
        }
        if emergency_signers.len() != EMERGENCY_SIGNER_COUNT {
            panic!("Emergency signer set must be exactly 3");
        }
        let signer_0 = emergency_signers.get(0).unwrap();
        let signer_1 = emergency_signers.get(1).unwrap();
        let signer_2 = emergency_signers.get(2).unwrap();
        if signer_0 == signer_1 || signer_0 == signer_2 || signer_1 == signer_2 {
            panic!("Emergency signers must be unique");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::XlmToken, &xlm_token);
        env.storage().instance().set(&DataKey::UsdcToken, &usdc_token);
        env.storage()
            .instance()
            .set(&DataKey::DefaultTtlLedgers, &default_ttl_ledgers);
        if let Some(reg) = registry {
            env.storage()
                .instance()
                .set(&DataKey::RegistryContract, &reg);
        }
        env.storage()
            .instance()
            .set(&DataKey::EmergencySigners, &emergency_signers);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyTreasury, &emergency_treasury);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyCooldownLedgers, &emergency_cooldown_ledgers);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyLastDrainLedger, &0u32);

        // Initialize the upgrade timelock delay to a safe default.
        env.storage()
            .instance()
            .set(&DataKey::UpgradeDelayLedgers, &DEFAULT_UPGRADE_DELAY_LEDGERS);

        // Initialize contract version to 1.
        env.storage().instance().set(&DataKey::Version, &1u32);
    }

    /// Schedule a WASM upgrade. Admin-only. The upgrade does not take effect
    /// immediately: it must wait out the configured timelock delay and then be
    /// applied via `apply_upgrade`. This gives affected parties time to notice
    /// and react before new code can control escrowed funds.
    pub fn schedule_upgrade(env: Env, caller: Address, new_wasm_hash: BytesN<32>) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let delay: u32 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeDelayLedgers)
            .unwrap_or(DEFAULT_UPGRADE_DELAY_LEDGERS);
        let apply_ledger = env.ledger().sequence().saturating_add(delay);

        env.storage()
            .instance()
            .set(&DataKey::PendingUpgradeHash, &new_wasm_hash);
        env.storage()
            .instance()
            .set(&DataKey::PendingUpgradeLedger, &apply_ledger);

        env.events().publish(
            ("ESCROW", "upgrade_scheduled"),
            UpgradeScheduledEvent {
                new_wasm_hash,
                apply_ledger,
            },
        );
    }

    /// Cancel a previously scheduled upgrade. Admin-only.
    pub fn cancel_upgrade(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let pending: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::PendingUpgradeHash)
            .unwrap_or_else(|| panic!("No pending upgrade"));

        env.storage().instance().remove(&DataKey::PendingUpgradeHash);
        env.storage().instance().remove(&DataKey::PendingUpgradeLedger);

        env.events().publish(
            ("ESCROW", "upgrade_cancelled"),
            UpgradeCancelledEvent {
                new_wasm_hash: pending,
            },
        );
    }

    /// Apply a previously scheduled upgrade once the timelock delay has
    /// elapsed. Admin-only. Reverts if no upgrade is pending or the delay has
    /// not yet passed.
    pub fn apply_upgrade(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let new_wasm_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::PendingUpgradeHash)
            .unwrap_or_else(|| panic!("No pending upgrade"));
        let apply_ledger: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PendingUpgradeLedger)
            .unwrap_or_else(|| panic!("No pending upgrade"));

        if env.ledger().sequence() < apply_ledger {
            panic!("Upgrade timelock has not elapsed");
        }

        env.storage().instance().remove(&DataKey::PendingUpgradeHash);
        env.storage().instance().remove(&DataKey::PendingUpgradeLedger);

        let version: u32 = env.storage().instance().get(&DataKey::Version).unwrap_or(0);
        env.storage().instance().set(&DataKey::Version, &(version + 1));

        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    /// Return the current contract version.
    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Version).unwrap_or(0)
    }

    /// Return the currently pending upgrade hash and its apply ledger, if any.
    pub fn get_pending_upgrade(env: Env) -> Option<(BytesN<32>, u32)> {
        let hash: Option<BytesN<32>> = env
            .storage()
            .instance()
            .get(&DataKey::PendingUpgradeHash);
        let apply_ledger: Option<u32> = env
            .storage()
            .instance()
            .get(&DataKey::PendingUpgradeLedger);
        match (hash, apply_ledger) {
            (Some(h), Some(l)) => Some((h, l)),
            _ => None,
        }
    }

    /// Update (or remove) the merchant registry address.  Admin-only.
    pub fn set_registry(env: Env, caller: Address, registry: Option<Address>) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        match registry {
            Some(reg) => env
                .storage()
                .instance()
                .set(&DataKey::RegistryContract, &reg),
            None => env.storage().instance().remove(&DataKey::RegistryContract),
        }
    }

    /// Return the registry contract address if one is configured.
    pub fn get_registry(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::RegistryContract)
    }

    pub fn deposit(
        env: Env,
        customer: Address,
        payment_id: BytesN<32>,
        merchant: Address,
        amount: i128,
        ttl_ledgers: u32,
        asset_type: AssetType,
    ) -> BytesN<32> {
        customer.require_auth();

        if amount <= 0 {
            panic!("Amount must be > 0");
        }
        if ttl_ledgers == 0 {
            panic!("TTL must be > 0");
        }
        if ttl_ledgers > MAX_TTL_LEDGERS {
            panic!("TTL exceeds maximum");
        }

        let key = DataKey::Payment(payment_id.clone());
        if env.storage().persistent().has(&key) {
            panic!("Payment ID already exists");
        }

        // If a registry contract is configured, verify the merchant is approved.
        if let Some(registry_addr) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::RegistryContract)
        {
            let registry_client = MerchantRegistryClient::new(&env, &registry_addr);
            if !registry_client.is_approved(&merchant) {
                panic!("Merchant is not approved");
            }
        }

        // Transfer funds into the contract via the appropriate token interface.
        let token_addr = Self::token_address(&env, &asset_type);
        token::Client::new(&env, &token_addr).transfer(
            &customer,
            &env.current_contract_address(),
            &amount,
        );

        let expiry = env.ledger().sequence().saturating_add(ttl_ledgers);
        let dispute_window_end = {
            let max_window_end = env
                .ledger()
                .sequence()
                .saturating_add(MAX_DISPUTE_WINDOW_LEDGERS);
            if expiry < max_window_end {
                expiry
            } else {
                max_window_end
            }
        };

        let payment = PaymentEscrow {
            payment_id: payment_id.clone(),
            amount,
            released_amount: 0,
            merchant: merchant.clone(),
            customer: customer.clone(),
            status: PaymentStatus::Pending,
            expiry,
            dispute_window_end,
            dispute_reason: None,
            asset_type,
        };

        env.storage().persistent().set(&key, &payment);

        env.events().publish(
            ("ESCROW", "deposit"),
            DepositEvent {
                payment_id: payment_id.clone(),
                customer,
                merchant,
                amount,
                expiry,
            },
        );

        payment_id
    }

    pub fn release(env: Env, caller: Address, payment_id: BytesN<32>) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let mut payment = Self::get_payment(env.clone(), payment_id.clone());
        Self::require_releasable(&env, &payment);

        let remaini

/* … truncated 12049 chars — edit only what you need near the top … */
