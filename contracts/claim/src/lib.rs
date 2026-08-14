//! ShieldedPay Claim Contract
//!
//! Entry point for a disbursement recipient to claim their payment.
//! Authorization is possession-based rather than identity-based, by
//! design: whoever holds a valid (leaf, proof) pair for a committed
//! payroll batch can claim it, matching the product's "unique claim link,
//! no signup needed" model (see frontend/backend READMEs). A `nullifier`
//! derived from the leaf prevents the same disbursement being claimed
//! twice; this contract's only state is the set of spent nullifiers.
//!
//! Proof validity is delegated to the Payroll contract's
//! `verify_disbursement`, called cross-contract so this contract never
//! needs its own copy of the committed Merkle roots.
//!
//! Releasing the actual funds to `recipient` (a token transfer, likely via
//! the Treasury contract) is intentionally not wired up yet -- see
//! ISSUE-BACKLOG.md. Today this contract proves eligibility and burns the
//! nullifier; it does not yet move funds.

#![no_std]
// TODO: migrate to the #[contractevent] macro (see ISSUE-BACKLOG.md) --
// events().publish() still works but is deprecated in soroban-sdk 27.x.
#![allow(deprecated)]

use shieldedpay_payroll::PayrollContractClient;
use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, BytesN, Env, Vec};

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Spent(BytesN<32>),
}

/// Bundled arguments for `claim_payment` -- kept as a single struct rather
/// than individual parameters (clippy's `too_many_arguments` lint, and
/// honestly just easier for a caller to construct correctly).
#[contracttype]
#[derive(Clone)]
pub struct ClaimRequest {
    pub payroll_contract: Address,
    pub payroll_id: u64,
    pub nullifier: BytesN<32>,
    pub leaf: BytesN<32>,
    pub proof: Vec<BytesN<32>>,
    pub index: u32,
    pub recipient: Address,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyClaimed = 1,
    InvalidProof = 2,
}

const LEDGER_BUMP: u32 = 120_960; // ~7 days at 5s/ledger
const LEDGER_THRESHOLD: u32 = 100_800; // ~6 days

#[contract]
pub struct ClaimContract;

#[contractimpl]
impl ClaimContract {
    pub fn check_nullifier(env: Env, nullifier: BytesN<32>) -> bool {
        env.storage().persistent().has(&DataKey::Spent(nullifier))
    }

    /// Verifies `request.leaf`/`proof`/`index` against the payroll batch
    /// identified by `request.payroll_id` on `request.payroll_contract`,
    /// and -- if valid and the nullifier hasn't been used -- marks the
    /// nullifier spent.
    ///
    /// `request.recipient` is recorded on the claim event for
    /// indexing/audit purposes; the actual transfer to `recipient` is not
    /// yet implemented here (see module docs).
    pub fn claim_payment(env: Env, request: ClaimRequest) -> Result<(), Error> {
        let key = DataKey::Spent(request.nullifier.clone());
        if env.storage().persistent().has(&key) {
            return Err(Error::AlreadyClaimed);
        }

        let payroll_client = PayrollContractClient::new(&env, &request.payroll_contract);
        let valid = payroll_client.verify_disbursement(
            &request.payroll_id,
            &request.leaf,
            &request.proof,
            &request.index,
        );
        if !valid {
            return Err(Error::InvalidProof);
        }

        env.storage().persistent().set(&key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);

        env.events().publish(
            (soroban_sdk::symbol_short!("claimed"), request.recipient),
            (request.payroll_id, request.nullifier),
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shieldedpay_payroll::PayrollContract;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Bytes;

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

    fn setup_payroll(env: &Env) -> (Address, Address, BytesN<32>, BytesN<32>, BytesN<32>) {
        let org = Address::generate(env);
        let payroll_id_contract = env.register(PayrollContract, ());
        let payroll_client = PayrollContractClient::new(env, &payroll_id_contract);

        let leaf_a = leaf_hash(env, "alice:100");
        let leaf_b = leaf_hash(env, "bob:200");
        let root = pair_hash(env, &leaf_a, &leaf_b);

        payroll_client.commit_payroll(&org, &1, &root, &2);

        (payroll_id_contract, org, leaf_a, leaf_b, root)
    }

    #[test]
    fn claim_with_valid_proof_succeeds_and_burns_nullifier() {
        let env = Env::default();
        env.mock_all_auths();
        let (payroll_addr, _org, leaf_a, leaf_b, _root) = setup_payroll(&env);

        let claim_id = env.register(ClaimContract, ());
        let claim_client = ClaimContractClient::new(&env, &claim_id);

        let recipient = Address::generate(&env);
        let nullifier = leaf_hash(&env, "nullifier-for-bob");
        let mut proof = Vec::new(&env);
        proof.push_back(leaf_a.clone());

        assert!(!claim_client.check_nullifier(&nullifier));

        claim_client.claim_payment(&ClaimRequest {
            payroll_contract: payroll_addr,
            payroll_id: 1,
            nullifier: nullifier.clone(),
            leaf: leaf_b,
            proof,
            index: 1,
            recipient,
        });

        assert!(claim_client.check_nullifier(&nullifier));
    }

    #[test]
    fn double_claim_with_same_nullifier_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (payroll_addr, _org, leaf_a, leaf_b, _root) = setup_payroll(&env);

        let claim_id = env.register(ClaimContract, ());
        let claim_client = ClaimContractClient::new(&env, &claim_id);

        let recipient = Address::generate(&env);
        let nullifier = leaf_hash(&env, "nullifier-for-bob");
        let mut proof = Vec::new(&env);
        proof.push_back(leaf_a.clone());

        let request = ClaimRequest {
            payroll_contract: payroll_addr,
            payroll_id: 1,
            nullifier,
            leaf: leaf_b,
            proof,
            index: 1,
            recipient,
        };

        claim_client.claim_payment(&request);

        let result = claim_client.try_claim_payment(&request);
        assert_eq!(result, Err(Ok(Error::AlreadyClaimed)));
    }

    #[test]
    fn claim_with_invalid_proof_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (payroll_addr, _org, _leaf_a, leaf_b, _root) = setup_payroll(&env);

        let claim_id = env.register(ClaimContract, ());
        let claim_client = ClaimContractClient::new(&env, &claim_id);

        let recipient = Address::generate(&env);
        let nullifier = leaf_hash(&env, "nullifier-for-bob");
        let bogus_sibling = leaf_hash(&env, "not-alice");
        let mut proof = Vec::new(&env);
        proof.push_back(bogus_sibling);

        let result = claim_client.try_claim_payment(&ClaimRequest {
            payroll_contract: payroll_addr,
            payroll_id: 1,
            nullifier,
            leaf: leaf_b,
            proof,
            index: 1,
            recipient,
        });
        assert_eq!(result, Err(Ok(Error::InvalidProof)));
    }
}
