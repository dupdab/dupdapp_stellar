#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

/// Storage keys for the contract registry.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// The admin allowed to update registry entries.
    Admin,
    /// Maps a contract name to its current address.
    Address(Symbol),
}

#[contract]
pub struct ContractRegistry;

#[contractimpl]
impl ContractRegistry {
    /// Initialize the registry with an admin who can update addresses.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.events()
            .publish(("CONTRACT_REGISTRY", "initialized"), (admin,));
    }

    /// Return the current admin.
    pub fn admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized")
    }

    /// Register or update the address for a named contract. Admin only.
    pub fn set_address(env: Env, name: Symbol, address: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::Address(name.clone()), &address);

        env.events()
            .publish((Symbol::new(&env, "address_set"), name), address);
    }

    /// Look up the address for a named contract.
    pub fn get_address(env: Env, name: Symbol) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::Address(name))
            .expect("address not registered")
    }

    /// Returns true if an address is registered for the given name.
    pub fn has_address(env: Env, name: Symbol) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Address(name))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    fn setup() -> (Env, ContractRegistryClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, ContractRegistry);
        let client = ContractRegistryClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, client, admin)
    }

    #[test]
    fn set_and_get_address() {
        let (env, client, _admin) = setup();
        let name = Symbol::new(&env, "payment_escrow");
        let addr = Address::generate(&env);
        client.set_address(&name, &addr);
        assert_eq!(client.get_address(&name), addr);
        assert!(client.has_address(&name));
    }

    #[test]
    fn admin_can_update_address() {
        let (env, client, _admin) = setup();
        let name = Symbol::new(&env, "fee_calculator");
        let first = Address::generate(&env);
        let second = Address::generate(&env);
        client.set_address(&name, &first);
        client.set_address(&name, &second);
        assert_eq!(client.get_address(&name), second);
    }

    #[test]
    fn stale_address_detection() {
        let (env, client, _admin) = setup();
        let name = Symbol::new(&env, "payment_escrow");
        let old_addr = Address::generate(&env);
        let new_addr = Address::generate(&env);

        client.set_address(&name, &old_addr);
        let cached = client.get_address(&name);

        // Admin redeploys and updates the registry.
        client.set_address(&name, &new_addr);
        let current = client.get_address(&name);

        // A consumer holding the stale cached address can detect the change.
        assert_ne!(cached, current);
        assert_eq!(current, new_addr);
    }

    #[test]
    #[should_panic]
    fn get_unregistered_panics() {
        let (env, client, _admin) = setup();
        client.get_address(&Symbol::new(&env, "missing"));
    }
}
