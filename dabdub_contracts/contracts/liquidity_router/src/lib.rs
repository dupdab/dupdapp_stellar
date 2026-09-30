#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};

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
            pools.push_back(pool);
            env.storage().persistent().set(&DataKey::ApprovedPools, &pools);
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
        for p in pools.iter() {
            if p != pool {
                new_pools.push_back(p);
            }
        }
        env.storage().persistent().set(&DataKey::ApprovedPools, &new_pools);
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

        // Issue #1086: Query AMM reserves via cross-contract call using the
        // fallible `try_get_reserves` pattern. A previously-approved pool may
        // have been upgraded to a contract that no longer implements
        // `AmmInterface`, or may simply revert. In that case we must not let
        // the raw panic propagate and break every `check_and_route` call.
        //
        // Expected behavior: a failed `get_reserves` call is treated the same
        // as a shallow pool — we fall back to `Route::StellarClassicDEX` and
        // emit `FallbackRouteUsed` (with reserve = 0, since no reserve could
        // be read) rather than propagating the error.
        let amm_client = AmmClient::new(&env, &pool_address);
        let reserves = match amm_client.try_get_reserves() {
            Ok(Ok((reserve_a, _reserve_b))) => Some(reserve_a),
            Ok(Err(_)) | Err(_) => None,
        };

        let reserve_a = match reserves {
            Some(r) => r,
            None => {
                // Cross-contract call failed or reverted: degrade gracefully to
                // the classic DEX fallback instead of panicking.
                env.events().publish(
                    (Symbol::new(&env, "FallbackRouteUsed"),),
                    FallbackRouteUsed {
                        pool_id: pool_address,
                        amount: amount_in,
                        reserve: 0,
                    },
                );
                return Route::StellarClassicDEX;
            }
        };

        // Issue #1081: Validate the cross-contract AMM response before trusting it.
        // A misbehaving or malicious pool could return a negative or nonsensical
        // reserve, which would otherwise be used uncritically in the depth check.
        if reserve_a <= 0 {
            panic!("invalid pool reserves");
        }

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
