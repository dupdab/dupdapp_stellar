#![no_std]

mod test;

use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, Address, Env, Vec,
};

// ── Role ────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    ReadOnly,
    ComplianceAdmin,
    OperationsAdmin,
    SuperAdmin,
}

use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, Address, Env, Vec,
};

// ── Role ────────────────────────────────────────────────────────────────────
    }
}
