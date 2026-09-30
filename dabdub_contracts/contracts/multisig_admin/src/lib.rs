#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, vec, Address, Bytes, Env, String, Vec};

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
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admins,
    Threshold,
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
        env.storage().instance().set(&DataKey::Threshold, &2u32);
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

    pub fn get_admins(env: Env) -> Vec<Address> {
        env.storage().instance().get(&DataKey::Admins).unwrap()
    }

    pub fn get_threshold(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Threshold).unwrap_or(2)
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> Option<Proposal> {
        env.storage().persistent().get(&DataKey::Proposal(proposal_id))
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admins: Vec<Address> = env.storage().instance().get(&DataKey::Admins).unwrap();
        if !Self::contains_address(&admins, caller) {
            panic!("Not admin");
        }
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
        if env.ledger().timestamp() > proposal.expires_at {
            panic!("proposal expired");
        }
        let threshold = Self::get_threshold(env.clone());
        if proposal.approvals.len() >= threshold {
            proposal.executed = true;
            Self::apply_operation(env, proposal);
            env.events().publish(
                ("MULTISIG", "proposal_executed"),
                ProposalExecutedEvent {
                    proposal_id: proposal.id,
                    operation: proposal.operation.clone(),
                },
            );
        }
    }

    fn apply_operation(env: &Env, proposal: &Proposal) {
        let op = proposal.operation.clone();
        if op == String::from_str(env, "add_admin") {
            let new_admin = Self::decode_address(env, &proposal.args);
            let mut admins: Vec<Address> = env.storage().instance().get(&DataKey::Admins).unwrap();
            if Self::contains_address(&admins, &new_admin) {
                panic!("admin already exists");
            }
            admins.push_back(new_admin);
            env.storage().instance().set(&DataKey::Admins, &admins);
        } else if op == String::from_str(env, "remove_admin") {
            let target = Self::decode_address(env, &proposal.args);
            let admins: Vec<Address> = env.storage().instance().get(&DataKey::Admins).unwrap();
            if !Self::contains_address(&admins, &target) {
                panic!("admin not found");
            }
            if admins.len() <= 1 {
                panic!("cannot remove last admin");
            }
            let mut updated = vec![env];
            for i in 0..admins.len() {
                let a = admins.get(i).unwrap();
                if a != target {
                    updated.push_back(a);
                }
            }
            let threshold = Self::get_threshold(env.clone());
            if threshold > updated.len() {
                panic!("threshold exceeds admin count");
            }
            env.storage().instance().set(&DataKey::Admins, &updated);
        } else if op == String::from_str(env, "set_threshold") {
            let new_threshold = Self::decode_u32(env, &proposal.args);
            let admins: Vec<Address> = env.storage().instance().get(&DataKey::Admins).unwrap();
            if new_threshold == 0 || new_threshold > admins.len() {
                panic!("invalid threshold");
            }
            env.storage().instance().set(&DataKey::Threshold, &new_threshold);
        }
    }

    fn decode_address(env: &Env, args: &Bytes) -> Address {
        let mut buf = [0u8; 32];
        if args.len() != 32 {
            panic!("invalid address args");
        }
        args.copy_into_slice(&mut buf);
        Address::from_string_bytes(&String::from_bytes(env, &Bytes::from_slice(env, &buf)))
    }

    fn decode_u32(env: &Env, args: &Bytes) -> u32 {
        let mut buf = [0u8; 4];
        if args.len() != 4 {
            panic!("invalid u32 args");
        }
        args.copy_into_slice(&mut buf);
        u32::from_be_bytes(buf)
    }
}
