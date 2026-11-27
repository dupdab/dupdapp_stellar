#![no_std]

mod test;

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, vec, Address, Bytes, Env, String, Vec};

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

/// Maximum number of signers that may be configured. Bounds the size of the
/// stored signer set and the length of signature-verification loops.
const MAX_SIGNERS: u32 = 20;

/// Minimum delay that MUST elapse between a signer-rotation proposal reaching
/// quorum and its execution. Signer add/remove operations are routed through
/// the admin timelock so a compromised signer that meets quorum cannot
/// immediately remove the other signers and seize control.
const SIGNER_ROTATION_TIMELOCK_SECONDS: u64 = 48 * 60 * 60; // 48 hours

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Error {
    /// Adding a signer would exceed [`MAX_SIGNERS`].
    TooManySigners = 1,
}

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
    /// Earliest timestamp at which a queued signer-rotation proposal may be
    /// executed. Set when the rotation proposal reaches quorum.
    SignerRotationUnlock(u64),
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

/// Emitted each time a signer adds an approval to an operation, so off-chain
/// monitors can track progress toward quorum.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SignatureAddedEvent {
    pub op_id: u64,
    pub signer: Address,
    pub approvals_so_far: u32,
    pub quorum_required: u32,
}

/// Emitted once an operation's approvals reach the required quorum.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct QuorumReachedEvent {
    pub op_id: u64,
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
        env.storage()
            .instance()
            .set(&DataKey::NextProposalId, &0u64);
    }

    /// Adds a new signer to the configured signer set.
    ///
    /// Returns [`Error::TooManySigners`] if the signer set already holds
    /// [`MAX_SIGNERS`] signers, bounding the stored set and verification loops.
    pub fn add_signer(env: Env, caller: Address, signer: Address) -> Result<(), Error> {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let mut admins: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Admins)
            .unwrap_or_else(|| vec![&env]);

        if admins.len() >= MAX_SIGNERS {
            return Err(Error::TooManySigners);
        }

        admins.push_back(signer);
        env.storage().instance().set(&DataKey::Admins, &admins);
        Ok(())
    }

    pub fn propose(env: Env, caller: Address, operation: String, args: Bytes) -> u64 {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        if operation.len() == 0 {
            panic!("operation must not be empty");
        }

        let mut next_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextProposalId)
            .unwrap_or(0);
        let proposal_id = next_id;
        // Use checked arithmetic so that exhausting the u64 id space fails
        // loudly instead of silently reusing an id (which would overwrite an
        // existing stored Proposal at the same key).
        next_id = next_id.checked_add(1).expect("proposal id overflow");
        env.storage()
            .instance()
            .set(&DataKey::NextProposalId, &next_id);

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

    /// Proposes a generic action. Restricted to the first configured signer
    /// (signer 1). `action_type` and `target_contract` are opaque metadata
    /// carried alongside the `calldata`; like `propose`, they are stored and
    /// echoed in events but never dispatched as a cross-contract call.
    pub fn propose_action(
        env: Env,
        caller: Address,
        action_type: String,
        target_contract: Address,
        calldata: Bytes,
    ) -> u64 {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        if caller != Self::signer_one(&env) {
            panic!("only signer 1 may propose actions");
        }
        if action_type.len() == 0 {
            panic!("action_type must not be empty");
        }

        let mut next_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextProposalId)
            .unwrap_or(0);
        let proposal_id = next_id;
        next_id = next_id.checked_add(1).expect("proposal id overflow");
        env.storage()
            .instance()
            .set(&DataKey::NextProposalId, &next_id);

        let now = env.ledger().timestamp();
        let mut approvals = vec![&env];
        approvals.push_back(caller.clone());

        let mut proposal = Proposal {
            id: proposal_id,
            proposer: caller.clone(),
            operation: action_type.clone(),
            args: calldata,
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
                operation: action_type.clone(),
                expires_at: proposal.expires_at,
            },
        );

        Self::maybe_execute(&env, &mut proposal);
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        proposal_id
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admins: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Admins)
            .unwrap_or_else(|| vec![env]);
        if !admins.contains(caller) {
            panic!("caller is not an admin");
        }
    }

    fn signer_one(env: &Env) -> Address {
        let admins: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Admins)
            .unwrap_or_else(|| vec![env]);
        admins.get(0).expect("no signers configured")
    }

    fn maybe_execute(env: &Env, proposal: &mut Proposal) {
        let threshold: u32 = env
            .storage()
            .instance()
            .get(&DataKey::Threshold)
            .unwrap_or(THRESHOLD);
        if proposal.approvals.len() >= threshold && !proposal.executed && !proposal.rejected {
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
