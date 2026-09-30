#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};

// Issue #1082: Upper bound on the approved pool allowlist to keep
// add_pool/remove_pool/check_and_route from scaling without limit.
const MAX_POOLS: u32 = 100;

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    SorobanAMM,
    StellarClassicDEX,
}

// Issue #1025: Add admin-managed pool allowlist
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    ApprovedPools,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FallbackRouteUsed {
    pub pool_id: Address,
    pub amount: i128,
    pub reserve: i128,
}

// Issue #1083: Event emitted when a pool is added to the allowlist
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PoolAddedEvent {
    pub pool_id: Address,
}

// Issue #1083: Event emitted when a pool is removed from the allowlist
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PoolRemovedEvent {
    pub pool_id: Address,
}

#[soroban_sdk::contractclient(name = "AmmClient")]
pub trait AmmInterface {
    fn get_reserves(env: Env) -> (i128, i128);
}

#[contract]
pub struct LiquidityRouter;

#[contractimpl]
impl LiquidityRouter {
    // Issue #1025: Constructor to initialize admin
    pub fn initialize(env: Env, admin: Address) {
        env.storage().persistent().set(&DataKey::Admin, &admin);
        let pools: Vec<Address> = Vec::new(&env);
        env.storage().persistent().set(&DataKey::ApprovedPools, &pools);
    }

    // Issue #1025: Add pool to allowlist (admin-gated)
    pub fn add_pool(env: Env, admin: Address, pool: Address) {
        admin.require_auth();

        // Verify caller is the admin
        let stored_admin: Address = env.storage().persistent().get(&DataKey::Admin)
            .expect("admin not set");
        if admin != stored_admin {
            panic!("only admin can add pools");
        }

        let mut pools: Vec<Address> = env.storage().persistent().get(&DataKey::ApprovedPools)
            .unwrap_or_else(|| Vec::new(&env));

        // Prevent duplicates
        if !pools.iter().any(|p| p == pool) {
            // Issue #1082: Enforce a maximum allowlist size before inserting
            if pools.len() >= MAX_POOLS {
                panic!("max pools reached");
            }
            pools.push_back(pool.clone());
            env.storage().persistent().set(&DataKey::ApprovedPools, &pools);

            // Issue #1083: Emit event for the allowlist change
            env.events().publish(
                (Symbol::new(&env, "PoolAddedEvent"),),
                PoolAddedEvent { pool_id: pool },
            );
        }
    }

    // Issue #1025: Remove pool from allowlist (admin-gated)
    pub fn remove_pool(env: Env, admin: Address, pool: Address) {
        admin.require_auth();

        // Verify caller is the admin
        let stored_admin: Address = env.storage().persistent().get(&DataKey::Admin)
            .expect("admin not set");
        if admin != stored_admin {
            panic!("only admin can remove pools");
        }

        let mut pools: Vec<Address> = env.storage().persistent().get(&DataKey::ApprovedPools)
            .unwrap_or_else(|| Vec::new(&env));

        // Remove the pool if it exists
        let mut new_pools = Vec::new(&env);
        let mut removed = false;
        for p in pools.iter() {
            if p != pool {
                new_pools.push_back(p);
            } else {
                removed = true;
            }
        }
        env.storage().persistent().set(&DataKey::ApprovedPools, &new_pools);

        // Issue #1083: Emit event only when a pool was actually removed
        if removed {
            env.events().publish(
                (Symbol::new(&env, "PoolRemovedEvent"),),
                PoolRemovedEvent { pool_id: pool },
            );
        }
    }

    /// Checks if the AMM pool has sufficient depth for a swap.
    /// Returns SorobanAMM if depth is sufficient (< 10% impact),
    /// otherwise returns StellarClassicDEX and emits an event.
    pub fn check_and_route(env: Env, pool_address: Address, amount_in: i128) -> Route {
        // Issue #1025: Validate pool_address against allowlist before calling get_reserves
        let pools: Vec<Address> = env.storage().persistent().get(&DataKey::ApprovedPools)
            .unwrap_or_else(|| Vec::new(&env));

        if !pools.iter().any(|p| p == pool_address) {
            panic!("pool is not approved");
        }

        // Query AMM reserves via cross-contract call
        let amm_client = AmmClient::new(&env, &pool_address);
        let (reserve_a, _reserve_b) = amm_client.get_reserves();

        // Depth check: amount_in must be less than 10% of reserves
        // S < R / 10
        if amount_in < reserve_a / 10 {
            Route::SorobanAMM
        } else {
            // Emit FallbackRouteUsed event
            env.events().publish(
                (Symbol::new(&env, "FallbackRouteUsed"),),
                FallbackRouteUsed {
                    pool_id: pool_address,
                    amount: amount_in,
                    reserve: reserve_a,
                },
            );
            Route::StellarClassicDEX
        }
    }
}
