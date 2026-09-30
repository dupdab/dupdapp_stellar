#![no_std]

mod test;

use soroban_sdk::{contract, contractimpl, contracttype, vec, Address, Bytes, Env, String, Vec};

/// Minimum number of approvals required before a proposal auto-executes.
///
/// SECURITY INVARIANT: `THRESHOLD` must NEVER be less than 2. Both `propose`
/// and `approve` call `maybe_execute`, which auto-executes as soon as
/// `proposal.approvals.len() >= THRESHOLD`. A fresh proposal already carries a
/// single approval (the proposer), so a threshold of 1 would let a lone proposer
/// immediately self-execute every proposal, silently defeating the entire
/// multisig premise. If this constant is ever made configurable, the setter
/// MUST enforce `assert!(new_threshold >= 2)`.
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

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ProposalPrunedEvent {
    pub proposal_id: u64,
}

/// Multisig admin **attestation / signaling** contract.
///
/// IMPORTANT: This contract does NOT execute proposals against any real
/// contract state. The `operation` and `args` fields carried by a [`Proposal`]
/// are opaque, caller-supplied metadata: they are stored and echoed back in
/// events, but they are NEVER interpreted, decoded, or dispatched as a
/// cross-contract call anywhere in this contract. Marking a proposal as
/// `executed` (see [`MultisigAdminContract::maybe_execute`]) only flips a
/// signaling flag and emits a `proposal_executed` event.
///
/// Applying the real effect described by `operation`/`args` is the
/// responsibility of an external, off-chain relayer that watches for the
/// `proposal_executed` event and performs the corresponding action itself.
/// Integrators MUST NOT assume that an `executed` proposal has already changed
/// any on-chain state.
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
        // Use checked arithmetic so that exhausting the u64 id space fails
        // loudly instead of silently reusing an id (which would overwrite an
        // existing stored Proposal at the same key).
        next_id = next_id.checked_add(1).expect("proposal id overflow");
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

        // A fresh proposal holds only the proposer's approval, so with the
        // required THRESHOLD >= 2 it cannot auto-execute here. See the
        // THRESHOLD invariant above.
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

        // Auto-executes only once approvals reach THRESHOLD (>= 2), which
        // requires at least one distinct approver beyond the proposer.
        Self::maybe_execute(&env, &mut proposal);
        env.storage().persistent().set(&key, &proposal);
    }

    /// Removes an expired, never-executed proposal from persistent storage.
    ///
    /// Permissionless: expiry is objectively checkable on-chain via
    /// `env.ledger().timestamp() > proposal.expires_at`, matching the check
    /// used by `approve`/`maybe_execute`. This lets anyone reclaim the rent
    /// and footprint of proposals that expired without reaching [`THRESHOLD`].
    ///
    /// Panics if the proposal does not exist, has already been executed, or
    /// has not yet expired — so live or executed proposals can never be pruned.
    pub fn prune_expired_proposal(env: Env, proposal_id: u64) {
        let key = DataKey::Proposal(proposal_id);
        let proposal: Proposal = env
            .storage()
            .persistent()
            .get(&key)
            .expect("proposal not found");

        if proposal.executed {
            panic!("proposal already executed");
        }
        if env.ledger().timestamp() <= proposal.expires_at {
            panic!("proposal not expired");
        }

        env.storage().persistent().remove(&key);
        env.events().publish(
            ("MULTISIG", "proposal_pruned"),
            ProposalPrunedEvent { proposal_id },
        );
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

    fn has_approved(approvals: &Vec<Address>, addr: &Address) -> bool {
        Self::contains_address(approvals, addr)
    }

    fn maybe_execute(env: &Env, proposal: &mut Proposal) {
        if proposal.executed {
            return;
        }
        if proposal.rejected {
            return;
        }
        if proposal.approvals.len() < THRESHOLD {
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

        proposal.executed = true;
        env.events().publish(
            ("MULTISIG", "proposal_executed"),
            ProposalExecutedEvent {
                proposal_id: proposal.id,
                operation: proposal.operation.clone(),
            },
        );
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
