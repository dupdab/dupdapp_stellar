#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, String, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaymentRequestStatus {
    Pending,
    Approved,
    Rejected,
    Cancelled,
    Settled,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRequest {
    pub id: u64,
    pub requester: Address,
    pub recipient: Address,
    pub amount: i128,
    pub asset: String,
    pub status: PaymentRequestStatus,
    pub memo: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    NextId,
    Request(u64),
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    NotFound = 4,
    InvalidStatus = 5,
}

#[contract]
pub struct PaymentRequestContract;

#[contractimpl]
impl PaymentRequestContract {
    /// Establishes the admin/settlement caller authorized to drive state
    /// transitions. Can only be called once.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::NextId, &0u64);
        env.events()
            .publish(("PAYMENT_REQUEST", "initialized"), (admin,));
        Ok(())
    }

    fn require_admin(env: &Env) -> Result<Address, Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        Ok(admin)
    }

    pub fn admin(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)
    }

    pub fn create_request(
        env: Env,
        requester: Address,
        recipient: Address,
        amount: i128,
        asset: String,
        memo: String,
    ) -> Result<u64, Error> {
        requester.require_auth();

        if amount <= 0 {
            panic!("amount must be > 0");
        }

        let id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextId)
            .ok_or(Error::NotInitialized)?;

        let request = PaymentRequest {
            id,
            requester,
            recipient,
            amount,
            asset,
            status: PaymentRequestStatus::Pending,
            memo,
        };

        env.storage().persistent().set(&DataKey::Request(id), &request);
        env.storage().instance().set(&DataKey::NextId, &(id + 1));
        env.events()
            .publish(("PAYMENT_REQUEST", "created"), (request,));

        Ok(id)
    }

    pub fn get_request(env: Env, id: u64) -> Result<PaymentRequest, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Request(id))
            .ok_or(Error::NotFound)
    }

    pub fn approve_request(env: Env, id: u64) -> Result<(), Error> {
        Self::require_admin(&env)?;
        Self::set_status(&env, id, PaymentRequestStatus::Approved)
    }

    pub fn reject_request(env: Env, id: u64) -> Result<(), Error> {
        Self::require_admin(&env)?;
        Self::set_status(&env, id, PaymentRequestStatus::Rejected)
    }

    pub fn settle_request(env: Env, id: u64) -> Result<(), Error> {
        Self::require_admin(&env)?;
        Self::set_status(&env, id, PaymentRequestStatus::Settled)
    }

    pub fn cancel_request(env: Env, id: u64) -> Result<(), Error> {
        let mut request: PaymentRequest = env
            .storage()
            .persistent()
            .get(&DataKey::Request(id))
            .ok_or(Error::NotFound)?;

        request.requester.require_auth();

        if request.status != PaymentRequestStatus::Pending {
            return Err(Error::InvalidStatus);
        }

        request.status = PaymentRequestStatus::Cancelled;
        env.storage().persistent().set(&DataKey::Request(id), &request);
        env.events().publish(
            ("PAYMENT_REQUEST", "status_updated"),
            (id, PaymentRequestStatus::Pending, PaymentRequestStatus::Cancelled),
        );
        Ok(())
    }

    fn set_status(env: &Env, id: u64, status: PaymentRequestStatus) -> Result<(), Error> {
        let mut request: PaymentRequest = env
            .storage()
            .persistent()
            .get(&DataKey::Request(id))
            .ok_or(Error::NotFound)?;

        let old_status = request.status.clone();
        request.status = status.clone();
        env.storage().persistent().set(&DataKey::Request(id), &request);
        env.events()
            .publish(("PAYMENT_REQUEST", "status_updated"), (id, old_status, status));
        Ok(())
    }

    pub fn list_requests(env: Env) -> Vec<PaymentRequest> {
        let next_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextId)
            .unwrap_or(0);

        let mut requests = Vec::new(env);
        let mut id: u64 = 0;
        while id < next_id {
            if let Some(request) = env
                .storage()
                .persistent()
                .get::<DataKey, PaymentRequest>(&DataKey::Request(id))
            {
                requests.push_back(request);
            }
            id += 1;
        }
        requests
    }
}
