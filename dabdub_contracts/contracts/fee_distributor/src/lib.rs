#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, Vec};

const BPS_DENOMINATOR: i128 = 10_000;
const MAX_FEE_RECIPIENTS: u32 = 10;
const RECIPIENT_UPDATE_DELAY_LEDGERS: u32 = 17_280;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeRecipient {
    pub address: Address,
    pub share_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeePayout {
    pub address: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingRecipientUpdate {
    pub recipients: Vec<FeeRecipient>,
    pub execute_after: u32,
}

#[contracttype]
enum DataKey {
    Admin,
    Recipients,
    UsdcToken,
    AllowedCaller,
    PendingRecipients,
}

#[contracttype]
struct FeeDistributedEvent {
    total_amount: i128,
    payouts: Vec<FeePayout>,
}

#[contract]
pub struct FeeDistributorContract;

#[contractimpl]
impl FeeDistributorContract {
    pub fn __constructor(
        env: Env,
        admin: Address,
        recipients: Vec<FeeRecipient>,
        usdc_token: Address,
        allowed_caller: Address,
    ) {
        Self::validate_recipients(&recipients);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Recipients, &recipients);
        env.storage()
            .instance()
            .set(&DataKey::UsdcToken, &usdc_token);
        env.storage()
            .instance()
            .set(&DataKey::AllowedCaller, &allowed_caller);
    }

    /// Pull and split a fee atomically. The final recipient receives any
    /// integer-division remainder so the full amount is always distributed.
    pub fn distribute(env: Env, caller: Address, total_amount: i128) {
        caller.require_auth();
        Self::require_allowed_caller(&env, &caller);
        assert!(total_amount > 0, "total_amount must be > 0");

        let recipients: Vec<FeeRecipient> = env
            .storage()
            .instance()
            .get(&DataKey::Recipients)
            .expect("recipients not configured");
        let usdc_token: Address = env.storage().instance().get(&DataKey::UsdcToken).unwrap();
        let token = token::Client::new(&env, &usdc_token);
        let contract_address = env.current_contract_address();
        token.transfer(&caller, &contract_address, &total_amount);

        let mut payouts = Vec::new(&env);
        let mut distributed = 0i128;
        for (index, recipient) in recipients.iter().enumerate() {
            let payout_amount = if index + 1 == recipients.len() as usize {
                total_amount
                    .checked_sub(distributed)
                    .expect("distributed amount exceeds total")
            } else {
                Self::share_amount(total_amount, recipient.share_bps)
            };

            if payout_amount > 0 {
                token.transfer(&contract_address, &recipient.address, &payout_amount);
            }
            distributed = distributed
                .checked_add(payout_amount)
                .expect("payout total overflow");
            payouts.push_back(FeePayout {
                address: recipient.address,
                amount: payout_amount,
            });
        }
        assert_eq!(
            distributed, total_amount,
            "recipient shares must total 10000 bps"
        );

        env.events().publish(
            ("FEE_DISTRIBUTOR", "fee_distributed"),
            FeeDistributedEvent {
                total_amount,
                payouts,
            },
        );
    }

    /// Schedule a recipient-list change. The change cannot be applied for
    /// 17,280 ledgers (~24 hours); no current recipient configuration changes
    /// until `apply_recipients` is called after that deadline.
    pub fn schedule_recipients(env: Env, caller: Address, recipients: Vec<FeeRecipient>) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        Self::validate_recipients(&recipients);
        let execute_after = env
            .ledger()
            .sequence()
            .checked_add(RECIPIENT_UPDATE_DELAY_LEDGERS)
            .expect("recipient update deadline overflow");
        let pending = PendingRecipientUpdate {
            recipients: recipients.clone(),
            execute_after,
        };
        env.storage().instance().extend_ttl(
            RECIPIENT_UPDATE_DELAY_LEDGERS,
            RECIPIENT_UPDATE_DELAY_LEDGERS,
        );
        env.storage()
            .instance()
            .set(&DataKey::PendingRecipients, &pending);
        env.events()
            .publish(("FEE_DISTRIBUTOR", "recipients_update_scheduled"), pending);
    }

    /// Apply a previously scheduled update once its timelock has elapsed.
    pub fn apply_recipients(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        let pending: PendingRecipientUpdate = env
            .storage()
            .instance()
            .get(&DataKey::PendingRecipients)
            .expect("no pending recipient update");
        if env.ledger().sequence() < pending.execute_after {
            panic!("recipient update timelock has not elapsed");
        }

        let previous: Vec<FeeRecipient> = env
            .storage()
            .instance()
            .get(&DataKey::Recipients)
            .expect("recipients not configured");
        env.storage()
            .instance()
            .set(&DataKey::Recipients, &pending.recipients);
        env.storage().instance().remove(&DataKey::PendingRecipients);
        env.events().publish(
            ("FEE_DISTRIBUTOR", "recipients_updated"),
            (previous, pending.recipients),
        );
    }

    /// Cancel a queued update before it is applied.
    pub fn cancel_recipients_update(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        if !env.storage().instance().has(&DataKey::PendingRecipients) {
            panic!("no pending recipient update");
        }
        env.storage().instance().remove(&DataKey::PendingRecipients);
        env.events()
            .publish(("FEE_DISTRIBUTOR", "recipients_update_cancelled"), ());
    }

    /// Rotate the address permitted to invoke `distribute`. Admin-only.
    pub fn set_allowed_caller(env: Env, caller: Address, allowed_caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        let previous: Address = env
            .storage()
            .instance()
            .get(&DataKey::AllowedCaller)
            .unwrap();
        env.storage()
            .instance()
            .set(&DataKey::AllowedCaller, &allowed_caller);
        env.events().publish(
            ("FEE_DISTRIBUTOR", "allowed_caller_updated"),
            (previous, allowed_caller),
        );
    }

    /// Rotate the admin address. Admin-only.
    pub fn transfer_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        let previous: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.events()
            .publish(("FEE_DISTRIBUTOR", "admin_updated"), (previous, new_admin));
    }

    pub fn get_recipients(env: Env) -> Vec<FeeRecipient> {
        env.storage()
            .instance()
            .get(&DataKey::Recipients)
            .expect("recipients not configured")
    }

    pub fn get_pending_recipients(env: Env) -> Option<PendingRecipientUpdate> {
        env.storage().instance().get(&DataKey::PendingRecipients)
    }

    pub fn get_allowed_caller(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::AllowedCaller)
            .unwrap()
    }

    fn validate_recipients(recipients: &Vec<FeeRecipient>) {
        if recipients.len() == 0 || recipients.len() > MAX_FEE_RECIPIENTS {
            panic!(
                "recipient count must be between 1 and {}",
                MAX_FEE_RECIPIENTS
            );
        }
        let mut total_share = 0u32;
        for (index, recipient) in recipients.iter().enumerate() {
            if recipient.share_bps == 0 {
                panic!("recipient share must be greater than zero");
            }
            for previous in recipients.iter().take(index) {
                if recipient.address == previous.address {
                    panic!("recipient addresses must be unique");
                }
            }
            total_share = total_share
                .checked_add(recipient.share_bps)
                .expect("recipient share total overflow");
        }
        if total_share != BPS_DENOMINATOR as u32 {
            panic!("recipient shares must total 10000 bps");
        }
    }

    fn share_amount(amount: i128, share_bps: u32) -> i128 {
        let whole = amount / BPS_DENOMINATOR;
        let remainder = amount % BPS_DENOMINATOR;
        whole
            .checked_mul(share_bps as i128)
            .and_then(|value| value.checked_add(remainder * share_bps as i128 / BPS_DENOMINATOR))
            .expect("recipient payout overflow")
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        assert!(caller == &admin, "not admin");
    }

    fn require_allowed_caller(env: &Env, caller: &Address) {
        let allowed: Address = env
            .storage()
            .instance()
            .get(&DataKey::AllowedCaller)
            .unwrap();
        assert!(caller == &allowed, "caller not allowed");
    }
}
