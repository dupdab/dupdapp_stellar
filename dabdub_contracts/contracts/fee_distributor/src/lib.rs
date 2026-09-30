#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env};

const BPS_DENOM: i128 = 10_000;

#[contracttype]
enum DataKey {
    Admin,
    Treasury,
    LpAddress,
    LpShareBps,
    UsdcToken,
    AllowedCaller,
}

#[contracttype]
struct FeeDistributedEvent {
    treasury_amount: i128,
    lp_amount: i128,
    lp_share_bps: i128,
}

#[contracttype]
struct LpShareUpdatedEvent {
    old_bps: i128,
    new_bps: i128,
}

#[contracttype]
struct TreasuryUpdatedEvent {
    old_treasury: Address,
    new_treasury: Address,
}

#[contracttype]
struct LpAddressUpdatedEvent {
    old_lp_address: Address,
    new_lp_address: Address,
}

#[contracttype]
struct AdminUpdatedEvent {
    old_admin: Address,
    new_admin: Address,
}

#[contract]
pub struct FeeDistributorContract;

#[contractimpl]
impl FeeDistributorContract {
    pub fn __constructor(
        env: Env,
        admin: Address,
        treasury: Address,
        lp_address: Address,
        lp_share_bps: i128,
        usdc_token: Address,
        allowed_caller: Address,
    ) {
        assert!(lp_share_bps >= 0 && lp_share_bps <= BPS_DENOM, "bps out of range");
        assert!(treasury != lp_address, "treasury and lp_address must differ");
        assert!(admin != treasury, "admin and treasury must differ");
        assert!(admin != lp_address, "admin and lp_address must differ");
        let contract_address = env.current_contract_address();
        assert!(treasury != contract_address, "treasury must not be the contract address");
        assert!(lp_address != contract_address, "lp_address must not be the contract address");
        assert!(admin != contract_address, "admin must not be the contract address");
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.storage().instance().set(&DataKey::LpAddress, &lp_address);
        env.storage().instance().set(&DataKey::LpShareBps, &lp_share_bps);
        env.storage().instance().set(&DataKey::UsdcToken, &usdc_token);
        env.storage().instance().set(&DataKey::AllowedCaller, &allowed_caller);
    }

    /// Splits `total_fee` between treasury and LP atomically.
    /// Only the configured `allowed_caller` may invoke this; the caller must
    /// have pre-approved this contract to transfer `total_fee` tokens.
    ///
    /// The caller's `require_auth()` (satisfied by the outer contract invocation
    /// being signed by them) is what authorizes `token.transfer` to move funds
    /// out of their account. No separate `approve` step is needed.
    pub fn distribute(env: Env, caller: Address, total_fee: i128) {
        caller.require_auth();
        Self::require_allowed_caller(&env, &caller);
        assert!(total_fee > 0, "total_fee must be > 0");

        let lp_share_bps: i128 = env.storage().instance().get(&DataKey::LpShareBps).unwrap();
        let treasury: Address = env.storage().instance().get(&DataKey::Treasury).unwrap();
        let lp_address: Address = env.storage().instance().get(&DataKey::LpAddress).unwrap();
        let usdc_token: Address = env.storage().instance().get(&DataKey::UsdcToken).unwrap();

        // Rounding policy: `lp_amount` truncates via integer division, so any
        // remainder from `total_fee * lp_share_bps / BPS_DENOM` is deliberately
        // allocated to the treasury (computed as `total_fee - lp_amount`).
        // This is an intentional policy choice favoring the treasury, not an
        // implementation detail — do not reorder the arithmetic to shift the
        // remainder to the LP without an explicit decision to change policy.
        let lp_amount = total_fee
            .checked_mul(lp_share_bps)
            .expect("overflow")
            .checked_div(BPS_DENOM)
            .expect("div zero");
        let treasury_amount = total_fee.checked_sub(lp_amount).expect("underflow");

        // Pull full fee from caller into this contract first, then push out.
        // Both outbound transfers happen in the same tx — atomicity is guaranteed
        // by Soroban's all-or-nothing execution model.
        let token = token::Client::new(&env, &usdc_token);
        token.transfer(&caller, &env.current_contract_address(), &total_fee);

        if treasury_amount > 0 {
            token.transfer(&env.current_contract_address(), &treasury, &treasury_amount);
        }
        if lp_amount > 0 {
            token.transfer(&env.current_contract_address(), &lp_address, &lp_amount);
        }

        env.events().publish(
            ("FEE_DISTRIBUTOR", "fee_distributed"),
            FeeDistributedEvent {
                treasury_amount,
                lp_amount,
                lp_share_bps,
            },
        );
    }

    /// Update LP share ratio. Admin-only.
    pub fn set_lp_share(env: Env, caller: Address, bps: i128) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        assert!(bps >= 0 && bps <= BPS_DENOM, "bps out of range");
        let old_bps: i128 = env.storage().instance().get(&DataKey::LpShareBps).unwrap();
        env.storage().instance().set(&DataKey::LpShareBps, &bps);
        env.events().publish(
            ("FEE_DISTRIBUTOR", "lp_share_updated"),
            LpShareUpdatedEvent { old_bps, new_bps: bps },
        );
    }

    /// Update treasury address. Admin-only.
    pub fn set_treasury(env: Env, caller: Address, treasury: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        let old_treasury: Address = env.storage().instance().get(&DataKey::Treasury).unwrap();
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.events().publish(
            ("FEE_DISTRIBUTOR", "treasury_updated"),
            TreasuryUpdatedEvent {
                old_treasury,
                new_treasury: treasury,
            },
        );
    }

    /// Update LP address. Admin-only.
    pub fn set_lp_address(env: Env, caller: Address, lp_address: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        let old_lp_address: Address = env.storage().instance().get(&DataKey::LpAddress).unwrap();
        env.storage().instance().set(&DataKey::LpAddress, &lp_address);
        env.events().publish(
            ("FEE_DISTRIBUTOR", "lp_address_updated"),
            LpAddressUpdatedEvent {
                old_lp_address,
                new_lp_address: lp_address,
            },
        );
    }

    /// Rotate the address permitted to call `distribute`. Admin-only.
    pub fn set_allowed_caller(env: Env, caller: Address, allowed_caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        env.storage().instance().set(&DataKey::AllowedCaller, &allowed_caller);
    }

    /// Rotate the admin address. Admin-only.
    /// Persists `new_admin` to `DataKey::Admin` so subsequent `require_admin`
    /// checks (set_lp_share, set_treasury, set_lp_address, set_allowed_caller,
    /// and future rotations) use the rotated key.
    pub fn transfer_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        let old_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.events().publish(
            ("FEE_DISTRIBUTOR", "admin_updated"),
            AdminUpdatedEvent {
                old_admin,
                new_admin,
            },
        );
    }

    pub fn get_config(env: Env) -> (Address, Address, i128) {
        let treasury: Address = env.storage().instance().get(&DataKey::Treasury).unwrap();
        let lp_address: Address = env.storage().instance().get(&DataKey::LpAddress).unwrap();
        let lp_share_bps: i128 = env.storage().instance().get(&DataKey::LpShareBps).unwrap();
        (treasury, lp_address, lp_share_bps)
    }

    pub fn get_allowed_caller(env: Env) -> Address {
        env.storage().instance().get(&DataKey::AllowedCaller).unwrap()
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        assert!(caller == &admin, "not admin");
    }

    fn require_allowed_caller(env: &Env, caller: &Address) {
        let allowed: Address = env.storage().instance().get(&DataKey::AllowedCaller).unwrap();
        assert!(caller == &allowed, "caller not allowed");
    }
}
