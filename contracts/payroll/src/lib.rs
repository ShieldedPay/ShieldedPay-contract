//! ShieldedPay Payroll Contract
//!
//! An organization commits a Merkle root over a batch of disbursements
//! (one leaf per employee payment: typically hash(recipient_commitment,
//! amount, salt)) without revealing the individual leaves on-chain. Anyone
//! holding a leaf and its Merkle proof can later prove that leaf was part
//! of a committed payroll via `verify_disbursement`, without the payroll
//! contract ever learning who else was paid or how much.
//!
//! The Claim contract is the actual entry point disbursement recipients
//! use; it calls `verify_disbursement` here as part of proving a claim is
//! legitimate before releasing funds and marking the nullifier spent.

#![no_std]
// TODO: migrate to the #[contractevent] macro (see ISSUE-BACKLOG.md) --
// events().publish() still works but is deprecated in soroban-sdk 27.x.
#![allow(deprecated)]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Bytes, BytesN, Env, Vec,
};

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Commitment(u64),
}

#[contracttype]
#[derive(Clone)]
pub struct PayrollCommitment {
    pub org: Address,
    pub merkle_root: BytesN<32>,
    pub employee_count: u32,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyCommitted = 1,
    NotFound = 2,
    EmptyBatch = 3,
}

const LEDGER_BUMP: u32 = 120_960; // ~7 days at 5s/ledger
const LEDGER_THRESHOLD: u32 = 100_800; // ~6 days

#[contract]
pub struct PayrollContract;

#[contractimpl]
impl PayrollContract {
    /// Org commits the Merkle root of a payroll batch. `payroll_id` must be
    /// unique per org (a simple incrementing counter or timestamp works);
    /// re-committing the same id fails rather than silently overwriting.
    pub fn commit_payroll(
        env: Env,
        org: Address,
        payroll_id: u64,
        merkle_root: BytesN<32>,
        employee_count: u32,
    ) -> Result<(), Error> {
        if employee_count == 0 {
            return Err(Error::EmptyBatch);
        }
        org.require_auth();

        let key = DataKey::Commitment(payroll_id);
        if env.storage().persistent().has(&key) {
            return Err(Error::AlreadyCommitted);
        }

        let commitment = PayrollCommitment {
            org: org.clone(),
            merkle_root,
            employee_count,
        };
        env.storage().persistent().set(&key, &commitment);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);

        env.events().publish(
            (soroban_sdk::symbol_short!("commit"), org),
            (payroll_id, employee_count),
        );

        Ok(())
    }

    pub fn get_commitment(env: Env, payroll_id: u64) -> Result<PayrollCommitment, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Commitment(payroll_id))
            .ok_or(Error::NotFound)
    }

    /// Verifies that `leaf` is part of the committed Merkle tree for
    /// `payroll_id`, given a sibling-hash `proof` and the leaf's `index` in
    /// the tree (index parity at each level determines hash ordering).
    /// Returns `false` (rather than erroring) for an unknown `payroll_id`
    /// or a proof that doesn't reconstruct the stored root -- callers
    /// (e.g. the claim contract) treat both as "not verified."
    pub fn verify_disbursement(
        env: Env,
        payroll_id: u64,
        leaf: BytesN<32>,
        proof: Vec<BytesN<32>>,
        index: u32,
    ) -> bool {
        let commitment: PayrollCommitment = match env
            .storage()
            .persistent()
            .get(&DataKey::Commitment(payroll_id))
        {
            Some(c) => c,
            None => return false,
        };

        let computed = Self::compute_root(&env, leaf, proof, index);
        computed == commitment.merkle_root
    }

    fn compute_root(env: &Env, leaf: BytesN<32>, proof: Vec<BytesN<32>>, index: u32) -> BytesN<32> {
        let mut computed = leaf;
        let mut idx = index;

        for sibling in proof.iter() {
            let mut buf = Bytes::new(env);
            if idx.is_multiple_of(2) {
                buf.append(&computed.clone().into());
                buf.append(&sibling.clone().into());
            } else {
                buf.append(&sibling.clone().into());
                buf.append(&computed.clone().into());
            }
            computed = env.crypto().sha256(&buf).into();
            idx /= 2;
        }

        computed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn leaf_hash(env: &Env, data: &str) -> BytesN<32> {
        let bytes = Bytes::from_slice(env, data.as_bytes());
        env.crypto().sha256(&bytes).into()
    }

    fn pair_hash(env: &Env, a: &BytesN<32>, b: &BytesN<32>) -> BytesN<32> {
        let mut buf = Bytes::new(env);
        buf.append(&a.clone().into());
        buf.append(&b.clone().into());
        env.crypto().sha256(&buf).into()
    }

    #[test]
    fn commit_and_verify_valid_proof() {
        let env = Env::default();
        env.mock_all_auths();
        let org = Address::generate(&env);
        let contract_id = env.register(PayrollContract, ());
        let client = PayrollContractClient::new(&env, &contract_id);

        // 4-leaf tree: leaves 0..3, index 1 = leaf_b
        let leaf_a = leaf_hash(&env, "alice:100");
        let leaf_b = leaf_hash(&env, "bob:200");
        let leaf_c = leaf_hash(&env, "carol:150");
        let leaf_d = leaf_hash(&env, "dave:50");

        let node_ab = pair_hash(&env, &leaf_a, &leaf_b);
        let node_cd = pair_hash(&env, &leaf_c, &leaf_d);
        let root = pair_hash(&env, &node_ab, &node_cd);

        client.commit_payroll(&org, &1, &root, &4);

        let mut proof = Vec::new(&env);
        proof.push_back(leaf_a.clone());
        proof.push_back(node_cd.clone());

        assert!(client.verify_disbursement(&1, &leaf_b, &proof, &1));
    }

    #[test]
    fn verify_rejects_wrong_leaf() {
        let env = Env::default();
        env.mock_all_auths();
        let org = Address::generate(&env);
        let contract_id = env.register(PayrollContract, ());
        let client = PayrollContractClient::new(&env, &contract_id);

        let leaf_a = leaf_hash(&env, "alice:100");
        let leaf_b = leaf_hash(&env, "bob:200");
        let root = pair_hash(&env, &leaf_a, &leaf_b);
        client.commit_payroll(&org, &1, &root, &2);

        let wrong_leaf = leaf_hash(&env, "mallory:999999");
        let mut proof = Vec::new(&env);
        proof.push_back(leaf_a.clone());

        assert!(!client.verify_disbursement(&1, &wrong_leaf, &proof, &1));
    }

    #[test]
    fn verify_unknown_payroll_id_returns_false() {
        let env = Env::default();
        let contract_id = env.register(PayrollContract, ());
        let client = PayrollContractClient::new(&env, &contract_id);

        let leaf = leaf_hash(&env, "alice:100");
        let proof = Vec::new(&env);
        assert!(!client.verify_disbursement(&999, &leaf, &proof, &0));
    }

    #[test]
    fn recommitting_same_id_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let org = Address::generate(&env);
        let contract_id = env.register(PayrollContract, ());
        let client = PayrollContractClient::new(&env, &contract_id);

        let root = leaf_hash(&env, "root1");
        client.commit_payroll(&org, &1, &root, &2);

        let root2 = leaf_hash(&env, "root2");
        let result = client.try_commit_payroll(&org, &1, &root2, &2);
        assert_eq!(result, Err(Ok(Error::AlreadyCommitted)));
    }
}
