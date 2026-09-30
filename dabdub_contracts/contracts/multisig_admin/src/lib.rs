#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, vec, Address, Bytes, Env, String, Vec};

const THRESHOLD: u32 = 2;
const EXPIRY_SECONDS: u64 = 24 * 60 * 60; // 24 hours

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub operation: String,
    pub args: Bytes,
    pub approvals: Vec<Address>,
    pub created_at: u64,
    pub expires_at: u64,
    pub executed: bool,
    pub rejected: bool,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admins,
    NextProposalId,
    Proposal(u64),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ProposalCreatedEvent {
    pub proposal_id: u64,
    pub proposer: Address,
    pub operation: String,
    pub expires_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ProposalApprovedEvent {
    pub proposal_id: u64,
    pub approver: Address,
    pub approvals: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ProposalRejectedEvent {
    pub proposal_id: u64,
    pub rejecter: Address,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ProposalExecutedEvent {
    pub proposal_id: u64,
    pub operation: String,
}

#[contract]
pub struct MultisigAdminContract;

#[contractimpl]
impl MultisigAdminContract {
    pub fn __constructor(env: Env, admin1: Address, admin2: Address, admin3: Address) {
        if admin1 == admin2 || admin1 == admin3 || admin2 == admin3 {
            panic!("admins must be unique");
        }

        let admins = vec![&env, admin1, admin2, admin3];
        env.storage().instance().set(&DataKey::Admins, &admins);
        env.storage().instance().set(&DataKey::NextProposalId, &0u64);
    }

    pub fn propose(env: Env, caller: Address, operation: String, args: Bytes) -> u64 {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        if operation.len() == 0 {
            panic!("operation must not be empty");
        }

        let mut next_id: u64 = env.storage().instance().get(&DataKey::NextProposalId).unwrap_or(0);
        let proposal_id = next_id;
        next_id = next_id.saturating_add(1);
        env.storage().instance().set(&DataKey::NextProposalId, &next_id);

        let now = env.ledger().timestamp();
        let mut approvals = vec![&env];
        approvals.push_back(caller.clone());

        let mut proposal = Proposal {
            id: proposal_id,
            proposer: caller.clone(),
            operation: operation.clone(),
            args,
            approvals,
            created_at: now,
            expires_at: now.saturating_add(EXPIRY_SECONDS),
            executed: false,
            rejected: false,
        };

        env.events().publish(
            ("MULTISIG", "proposal_created"),
            ProposalCreatedEvent {
                proposal_id,
                proposer: caller,
                operation: operation.clone(),
                expires_at: proposal.expires_at,
            },
        );

        Self::maybe_execute(&env, &mut proposal);
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        proposal_id
    }

    pub fn approve(env: Env, caller: Address, proposal_id: u64) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let key = DataKey::Proposal(proposal_id);
        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&key)
            .expect("proposal not found");

        if env.ledger().timestamp() > proposal.expires_at {
            panic!("proposal expired");
        }
        if proposal.executed {
            panic!("proposal already executed");
        }
        if proposal.rejected {
            panic!("proposal already rejected");
        }
        if caller == proposal.proposer {
            panic!("proposer cannot approve twice");
        }
        if Self::has_approved(&proposal.approvals, &caller) {
            panic!("already approved");
        }

        proposal.approvals.push_back(caller.clone());
        env.events().publish(
            ("MULTISIG", "proposal_approved"),
            ProposalApprovedEvent {
                proposal_id,
                approver: caller,
                approvals: proposal.approvals.len(),
            },
        );

        Self::maybe_execute(&env, &mut proposal);
        env.storage().persistent().set(&key, &proposal);
    }

    pub fn reject(env: Env, caller: Address, proposal_id: u64) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let key = DataKey::Proposal(proposal_id);
        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&key)
            .expect("proposal not found");

        if proposal.executed {
            panic!("proposal already executed");
        }
        if proposal.rejected {
            panic!("proposal already rejected");
        }

        proposal.rejected = true;
        env.events().publish(
            ("MULTISIG", "proposal_rejected"),
            ProposalRejectedEvent {
                proposal_id,
                rejecter: caller,
            },
        );

        env.storage().persistent().set(&key, &proposal);
    }

    pub fn get_admins(env: Env) -> Vec<Address> {
        env.storage().instance().get(&DataKey::Admins).unwrap()
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> Option<Proposal> {
        env.storage().persistent().get(&DataKey::Proposal(proposal_id))
    }

    fn require_admin(env: &Env, caller: &Address) {
        if !Self::is_admin(env, caller) {
            panic!("Not admin");
        }
    }

    fn is_admin(env: &Env, caller: &Address) -> bool {
        let admins: Vec<Address> = env.storage().instance().get(&DataKey::Admins).unwrap();
        Self::contains_address(&admins, caller)
    }

    fn contains_address(list: &Vec<Address>, addr: &Address) -> bool {
        for i in 0..list.len() {
            if list.get(i).unwrap() == *addr {
                return true;
            }
        }
        false
    }

    fn has_approved(approvals: &Vec<Address>, caller: &Address) -> bool {
        Self::contains_address(approvals, caller)
    }

    fn maybe_execute(env: &Env, proposal: &mut Proposal) {
        if proposal.executed {
            return;
        }
        if proposal.rejected {
            return;
        }
        if env.ledger().timestamp() > proposal.expires_at {
            panic!("proposal expired");
        }
        if proposal.approvals.len() >= THRESHOLD {
            proposal.executed = true;
            env.events().publish(
                ("MULTISIG", "proposal_executed"),
                ProposalExecutedEvent {
                    proposal_id: proposal.id,
                    operation: proposal.operation.clone(),
                },
            );
        }
    }
}
