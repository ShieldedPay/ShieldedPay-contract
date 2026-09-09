//! ShieldedPay Payroll Contract
//!
//! An organization commits a Merkle root over a batch of disbursements
//! (one leaf per employee payment: hash(recipient_commitment, amount, token, salt))
//! with domain separation. Anyone holding a leaf and its Merkle proof can
//! later prove that leaf was part of a committed payroll via `verify_disbursement`.

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, xdr::ToXdr, Address, Bytes,
    BytesN, Env, Vec,
};

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Commitment(u64),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayrollCommitment {
    pub org: Address,
    pub merkle_root: BytesN<32>,
    pub employee_count: u32,
    pub expiration_timestamp: u64,
    pub reclaimed: bool,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayrollCommittedEvent {
    pub org: Address,
    pub payroll_id: u64,
    pub employee_count: u32,
    pub expiration_timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundsReclaimedEvent {
    pub org: Address,
    pub payroll_id: u64,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyCommitted = 1,
    NotFound = 2,
    EmptyBatch = 3,
    BatchNotExpired = 4,
    AlreadyReclaimed = 5,
    Unauthorized = 6,
    BatchExpired = 7,
}

const LEDGER_BUMP: u32 = 120_960; // ~7 days at 5s/ledger
const LEDGER_THRESHOLD: u32 = 100_800; // ~6 days
pub const DEFAULT_EXPIRATION_WINDOW: u64 = 90 * 86_400; // 90 days in seconds

#[contract]
pub struct PayrollContract;

#[contractimpl]
impl PayrollContract {
    /// Org commits the Merkle root of a payroll batch with an optional expiration timestamp.
    pub fn commit_payroll(
        env: Env,
        org: Address,
        payroll_id: u64,
        merkle_root: BytesN<32>,
        employee_count: u32,
        expiration_timestamp: Option<u64>,
    ) -> Result<(), Error> {
        if employee_count == 0 {
            return Err(Error::EmptyBatch);
        }
        org.require_auth();

        let key = DataKey::Commitment(payroll_id);
        if env.storage().persistent().has(&key) {
            return Err(Error::AlreadyCommitted);
        }

        let expiration = match expiration_timestamp {
            Some(exp) if exp > 0 => exp,
            _ => env.ledger().timestamp() + DEFAULT_EXPIRATION_WINDOW,
        };

        let commitment = PayrollCommitment {
            org: org.clone(),
            merkle_root,
            employee_count,
            expiration_timestamp: expiration,
            reclaimed: false,
        };
        env.storage().persistent().set(&key, &commitment);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);

        PayrollCommittedEvent {
            org,
            payroll_id,
            employee_count,
            expiration_timestamp: expiration,
        }
        .publish(&env);

        Ok(())
    }

    pub fn get_commitment(env: Env, payroll_id: u64) -> Result<PayrollCommitment, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Commitment(payroll_id))
            .ok_or(Error::NotFound)
    }

    /// Organization reclaims unclaimed payroll funds after expiration.
    pub fn reclaim_unclaimed_funds(env: Env, org: Address, payroll_id: u64) -> Result<(), Error> {
        org.require_auth();

        let key = DataKey::Commitment(payroll_id);
        let mut commitment: PayrollCommitment = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::NotFound)?;

        if commitment.org != org {
            return Err(Error::Unauthorized);
        }
        if commitment.reclaimed {
            return Err(Error::AlreadyReclaimed);
        }
        if env.ledger().timestamp() < commitment.expiration_timestamp {
            return Err(Error::BatchNotExpired);
        }

        commitment.reclaimed = true;
        env.storage().persistent().set(&key, &commitment);

        FundsReclaimedEvent { org, payroll_id }.publish(&env);

        Ok(())
    }

    /// Verifies that `leaf` is part of the committed Merkle tree for `payroll_id`
    /// enforcing domain separation and time-lock bounds.
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

        // If batch was reclaimed or has expired, claims are rejected
        if commitment.reclaimed {
            return false;
        }
        if env.ledger().timestamp() > commitment.expiration_timestamp {
            return false;
        }

        let computed = Self::compute_root(&env, leaf, proof, index);
        computed == commitment.merkle_root
    }

    /// Computes Merkle root with branch domain separation (0x01 prefix)
    pub fn compute_root(
        env: &Env,
        leaf: BytesN<32>,
        proof: Vec<BytesN<32>>,
        index: u32,
    ) -> BytesN<32> {
        let mut computed = leaf;
        let mut idx = index;

        for sibling in proof.iter() {
            let mut buf = Bytes::new(env);
            // Prepend 0x01 domain separation prefix for branch / interior nodes
            buf.push_back(0x01);
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

    /// Multi-token leaf hash computation with leaf domain separation (0x00 prefix).
    /// H(0x00, recipient, amount, token, salt)
    pub fn hash_disbursement_leaf(
        env: Env,
        recipient: Address,
        amount: i128,
        token: Address,
        salt: BytesN<32>,
    ) -> BytesN<32> {
        let mut buf = Bytes::new(&env);
        // Prepend 0x00 domain separation prefix for leaf nodes
        buf.push_back(0x00);
        buf.append(&recipient.to_xdr(&env));
        buf.append(&amount.to_xdr(&env));
        buf.append(&token.to_xdr(&env));
        buf.append(&salt.into());
        env.crypto().sha256(&buf).into()
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};

    pub fn leaf_hash(env: &Env, data: &str) -> BytesN<32> {
        let mut buf = Bytes::new(env);
        buf.push_back(0x00);
        let bytes = Bytes::from_slice(env, data.as_bytes());
        buf.append(&bytes);
        env.crypto().sha256(&buf).into()
    }

    pub fn pair_hash(env: &Env, a: &BytesN<32>, b: &BytesN<32>) -> BytesN<32> {
        let mut buf = Bytes::new(env);
        buf.push_back(0x01);
        buf.append(&a.clone().into());
        buf.append(&b.clone().into());
        env.crypto().sha256(&buf).into()
    }

    fn setup(env: &Env) -> (PayrollContractClient<'_>, Address) {
        let org = Address::generate(env);
        let contract_id = env.register(PayrollContract, ());
        let client = PayrollContractClient::new(env, &contract_id);
        (client, org)
    }

    #[test]
    fn recommitting_same_id_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, org) = setup(&env);

        let root = leaf_hash(&env, "dummy");
        client.commit_payroll(&org, &1, &root, &5, &None);

        let res = client.try_commit_payroll(&org, &1, &root, &5, &None);
        assert_eq!(res, Err(Ok(Error::AlreadyCommitted)));
    }

    #[test]
    fn verify_unknown_payroll_id_returns_false() {
        let env = Env::default();
        let (client, _) = setup(&env);

        let leaf = leaf_hash(&env, "dummy");
        let proof = Vec::new(&env);
        assert!(!client.verify_disbursement(&999, &leaf, &proof, &0));
    }

    #[test]
    fn commit_and_verify_valid_proof() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, org) = setup(&env);

        let leaf_a = leaf_hash(&env, "alice:100");
        let leaf_b = leaf_hash(&env, "bob:200");
        let root = pair_hash(&env, &leaf_a, &leaf_b);

        client.commit_payroll(&org, &42, &root, &2, &None);

        // Verify leaf A (index 0, sibling is B)
        let mut proof_a = Vec::new(&env);
        proof_a.push_back(leaf_b.clone());
        assert!(client.verify_disbursement(&42, &leaf_a, &proof_a, &0));

        // Verify leaf B (index 1, sibling is A)
        let mut proof_b = Vec::new(&env);
        proof_b.push_back(leaf_a.clone());
        assert!(client.verify_disbursement(&42, &leaf_b, &proof_b, &1));
    }

    #[test]
    fn verify_rejects_wrong_leaf() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, org) = setup(&env);

        let leaf_a = leaf_hash(&env, "alice:100");
        let leaf_b = leaf_hash(&env, "bob:200");
        let root = pair_hash(&env, &leaf_a, &leaf_b);

        client.commit_payroll(&org, &1, &root, &2, &None);

        let wrong_leaf = leaf_hash(&env, "eve:999");
        let mut proof = Vec::new(&env);
        proof.push_back(leaf_b);
        assert!(!client.verify_disbursement(&1, &wrong_leaf, &proof, &0));
    }

    #[test]
    fn test_multi_token_leaf_commitments() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, org) = setup(&env);

        let usdc = Address::generate(&env);
        let eurc = Address::generate(&env);
        let xlm = Address::generate(&env);

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let charlie = Address::generate(&env);

        let salt_a = leaf_hash(&env, "salt-alice");
        let salt_b = leaf_hash(&env, "salt-bob");
        let salt_c = leaf_hash(&env, "salt-charlie");

        let leaf_a = client.hash_disbursement_leaf(&alice, &5_000, &usdc, &salt_a);
        let leaf_b = client.hash_disbursement_leaf(&bob, &4_500, &eurc, &salt_b);
        let leaf_c = client.hash_disbursement_leaf(&charlie, &10_000, &xlm, &salt_c);

        // Build 4-leaf Merkle tree (with duplicated leaf_c for balance)
        let parent_ab = pair_hash(&env, &leaf_a, &leaf_b);
        let parent_cc = pair_hash(&env, &leaf_c, &leaf_c);
        let root = pair_hash(&env, &parent_ab, &parent_cc);

        client.commit_payroll(&org, &101, &root, &3, &None);

        // Verify Bob (leaf_b, index 1)
        // Level 0 sibling is leaf_a, Level 1 sibling is parent_cc
        let mut proof_b = Vec::new(&env);
        proof_b.push_back(leaf_a.clone());
        proof_b.push_back(parent_cc.clone());
        assert!(client.verify_disbursement(&101, &leaf_b, &proof_b, &1));

        // Verify Charlie (leaf_c, index 2)
        // Level 0 sibling is leaf_c, Level 1 sibling is parent_ab
        let mut proof_c = Vec::new(&env);
        proof_c.push_back(leaf_c.clone());
        proof_c.push_back(parent_ab.clone());
        assert!(client.verify_disbursement(&101, &leaf_c, &proof_c, &2));
    }

    #[test]
    fn test_batch_expiration_and_reclaim() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, org) = setup(&env);

        env.ledger().set_timestamp(1_000_000);
        let expiration = 1_000_000 + 10_000;

        let leaf_a = leaf_hash(&env, "alice:100");
        let leaf_b = leaf_hash(&env, "bob:200");
        let root = pair_hash(&env, &leaf_a, &leaf_b);

        client.commit_payroll(&org, &77, &root, &2, &Some(expiration));

        let mut proof = Vec::new(&env);
        proof.push_back(leaf_b);

        // Valid before expiration
        assert!(client.verify_disbursement(&77, &leaf_a, &proof, &0));

        // Cannot reclaim before expiration
        let err_early = client.try_reclaim_unclaimed_funds(&org, &77);
        assert_eq!(err_early, Err(Ok(Error::BatchNotExpired)));

        // Advance ledger beyond expiration
        env.ledger().set_timestamp(expiration + 1);

        // Claim fails after expiration
        assert!(!client.verify_disbursement(&77, &leaf_a, &proof, &0));

        // Org admin successfully reclaims
        client.reclaim_unclaimed_funds(&org, &77);

        // Reclaiming again fails
        let err_again = client.try_reclaim_unclaimed_funds(&org, &77);
        assert_eq!(err_again, Err(Ok(Error::AlreadyReclaimed)));
    }

    #[test]
    fn test_odd_length_tree_and_invalid_path_rejection() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, org) = setup(&env);

        let leaf_1 = leaf_hash(&env, "1");
        let leaf_2 = leaf_hash(&env, "2");
        let leaf_3 = leaf_hash(&env, "3");

        // 3 leaves: leaf_1 + leaf_2 -> parent_12, leaf_3 duplicated -> parent_33
        let parent_12 = pair_hash(&env, &leaf_1, &leaf_2);
        let parent_33 = pair_hash(&env, &leaf_3, &leaf_3);
        let root = pair_hash(&env, &parent_12, &parent_33);

        client.commit_payroll(&org, &88, &root, &3, &None);

        // Inverted sibling order should fail
        let mut inverted_proof = Vec::new(&env);
        inverted_proof.push_back(parent_33.clone());
        inverted_proof.push_back(leaf_2.clone());
        assert!(!client.verify_disbursement(&88, &leaf_1, &inverted_proof, &0));

        // Zero-hash salt / empty proof should fail
        let empty_proof = Vec::new(&env);
        assert!(!client.verify_disbursement(&88, &leaf_1, &empty_proof, &0));
    }
}
