//! ShieldedPay Claim Contract
//!
//! Entry point for a disbursement recipient to claim their payment.
//! Authorization is possession-based: whoever holds a valid (leaf, proof)
//! pair for a committed payroll batch can claim it. A `nullifier` prevents
//! double-claiming. Real token transfers execute atomically upon valid claim verification.

#![no_std]

use shieldedpay_payroll::PayrollContractClient;
use shieldedpay_treasury::TreasuryContractClient;
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, token, Address, BytesN,
    Env, Vec,
};

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Spent(BytesN<32>),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimRequest {
    pub payroll_contract: Address,
    pub payroll_id: u64,
    pub nullifier: BytesN<32>,
    pub leaf: BytesN<32>,
    pub proof: Vec<BytesN<32>>,
    pub index: u32,
    pub recipient: Address,
    pub amount: i128,
    pub token: Option<Address>,
    pub treasury_contract: Option<Address>,
    pub org: Option<Address>,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimedEvent {
    pub recipient: Address,
    pub payroll_id: u64,
    pub nullifier: BytesN<32>,
    pub amount: i128,
    pub token: Option<Address>,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyClaimed = 1,
    InvalidProof = 2,
    InvalidAmount = 3,
    TransferFailed = 4,
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
    /// identified by `request.payroll_id` on `request.payroll_contract`.
    /// Upon verification, atomically transfers funds to `recipient` via
    /// Treasury contract or direct token transfer and marks nullifier spent.
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

        // Execute SAC token transfer to recipient
        if request.amount > 0 {
            if let Some(ref treasury_addr) = request.treasury_contract {
                let treasury_client = TreasuryContractClient::new(&env, treasury_addr);
                let org = request
                    .org
                    .clone()
                    .unwrap_or_else(|| request.recipient.clone());
                treasury_client.disburse(&org, &request.recipient, &request.amount, &request.token);
            } else if let Some(ref token_addr) = request.token {
                let client = token::Client::new(&env, token_addr);
                client.transfer(
                    &env.current_contract_address(),
                    &request.recipient,
                    &request.amount,
                );
            }
        }

        // Mark nullifier spent
        env.storage().persistent().set(&key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);

        ClaimedEvent {
            recipient: request.recipient,
            payroll_id: request.payroll_id,
            nullifier: request.nullifier,
            amount: request.amount,
            token: request.token,
        }
        .publish(&env);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shieldedpay_payroll::PayrollContract;
    use shieldedpay_treasury::TreasuryContract;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Bytes;

    fn leaf_hash(env: &Env, data: &str) -> BytesN<32> {
        let mut buf = Bytes::new(env);
        buf.push_back(0x00);
        let bytes = Bytes::from_slice(env, data.as_bytes());
        buf.append(&bytes);
        env.crypto().sha256(&buf).into()
    }

    fn pair_hash(env: &Env, a: &BytesN<32>, b: &BytesN<32>) -> BytesN<32> {
        let mut buf = Bytes::new(env);
        buf.push_back(0x01);
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

        payroll_client.commit_payroll(&org, &1, &root, &2, &None);

        (payroll_id_contract, org, leaf_a, leaf_b, root)
    }

    #[test]
    fn claim_with_valid_proof_succeeds_and_burns_nullifier() {
        let env = Env::default();
        env.mock_all_auths();
        let (payroll_addr, org, leaf_a, leaf_b, _root) = setup_payroll(&env);

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
            amount: 0,
            token: None,
            treasury_contract: None,
            org: Some(org),
        });

        assert!(claim_client.check_nullifier(&nullifier));
    }

    #[test]
    fn double_claim_with_same_nullifier_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (payroll_addr, org, leaf_a, leaf_b, _) = setup_payroll(&env);

        let claim_id = env.register(ClaimContract, ());
        let claim_client = ClaimContractClient::new(&env, &claim_id);

        let recipient = Address::generate(&env);
        let nullifier = leaf_hash(&env, "nullifier-once");
        let mut proof = Vec::new(&env);
        proof.push_back(leaf_b);

        let req = ClaimRequest {
            payroll_contract: payroll_addr,
            payroll_id: 1,
            nullifier: nullifier.clone(),
            leaf: leaf_a,
            proof,
            index: 0,
            recipient,
            amount: 0,
            token: None,
            treasury_contract: None,
            org: Some(org),
        };

        claim_client.claim_payment(&req);
        let res = claim_client.try_claim_payment(&req);
        assert_eq!(res, Err(Ok(Error::AlreadyClaimed)));
    }

    #[test]
    fn claim_with_invalid_proof_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (payroll_addr, org, _leaf_a, leaf_b, _) = setup_payroll(&env);

        let claim_id = env.register(ClaimContract, ());
        let claim_client = ClaimContractClient::new(&env, &claim_id);

        let recipient = Address::generate(&env);
        let nullifier = leaf_hash(&env, "nullifier-bogus");
        let bogus_proof = Vec::new(&env);

        let res = claim_client.try_claim_payment(&ClaimRequest {
            payroll_contract: payroll_addr,
            payroll_id: 1,
            nullifier,
            leaf: leaf_b,
            proof: bogus_proof,
            index: 1,
            recipient,
            amount: 0,
            token: None,
            treasury_contract: None,
            org: Some(org),
        });

        assert_eq!(res, Err(Ok(Error::InvalidProof)));
    }

    #[test]
    fn test_claim_with_sac_token_disbursement_transfers_balance() {
        let env = Env::default();
        env.mock_all_auths();

        // 1. Setup SAC Token
        let token_admin = Address::generate(&env);
        let sac_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_address = sac_contract.address();
        let token_client = token::Client::new(&env, &token_address);
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

        // 2. Setup Treasury Contract
        let treasury_id = env.register(TreasuryContract, ());
        let treasury_client = TreasuryContractClient::new(&env, &treasury_id);
        let admin = Address::generate(&env);
        let org = Address::generate(&env);
        treasury_client.initialize(&admin, &Some(token_address.clone()));

        // Mint and deposit funds into treasury
        token_admin_client.mint(&org, &50_000);
        treasury_client.deposit(&org, &20_000);
        assert_eq!(token_client.balance(&treasury_id), 20_000);

        // 3. Setup Payroll Contract
        let payroll_id_contract = env.register(PayrollContract, ());
        let payroll_client = PayrollContractClient::new(&env, &payroll_id_contract);

        let recipient = Address::generate(&env);
        let salt = leaf_hash(&env, "bob-salt");
        let leaf_bob =
            payroll_client.hash_disbursement_leaf(&recipient, &3_500, &token_address, &salt);
        let leaf_other = leaf_hash(&env, "alice-leaf");
        let root = pair_hash(&env, &leaf_other, &leaf_bob);

        payroll_client.commit_payroll(&org, &99, &root, &2, &None);

        // 4. Setup Claim Contract
        let claim_id = env.register(ClaimContract, ());
        let claim_client = ClaimContractClient::new(&env, &claim_id);

        let nullifier = leaf_hash(&env, "nullifier-bob-99");
        let mut proof = Vec::new(&env);
        proof.push_back(leaf_other);

        assert_eq!(token_client.balance(&recipient), 0);

        // Claim payment with token disbursement
        claim_client.claim_payment(&ClaimRequest {
            payroll_contract: payroll_id_contract,
            payroll_id: 99,
            nullifier: nullifier.clone(),
            leaf: leaf_bob,
            proof,
            index: 1,
            recipient: recipient.clone(),
            amount: 3_500,
            token: Some(token_address.clone()),
            treasury_contract: Some(treasury_id.clone()),
            org: Some(org.clone()),
        });

        // Verify recipient received SAC tokens
        assert_eq!(token_client.balance(&recipient), 3_500);
        // Verify treasury balance decreased
        assert_eq!(token_client.balance(&treasury_id), 16_500);
        assert_eq!(treasury_client.balance(&org), 16_500);
        // Verify nullifier burned
        assert!(claim_client.check_nullifier(&nullifier));
    }
}
