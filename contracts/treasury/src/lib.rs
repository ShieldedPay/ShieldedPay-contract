//! ShieldedPay Treasury Contract
//!
//! Holds each organization's payroll funds and tracks per-organization
//! balances on-chain. `deposit`/`withdraw` move an org's ledger balance
//! and transfer real Stellar tokens using `token::Client`.

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, token, Address, Env,
};

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    DefaultToken,
    Balance(Address),
    TokenBalance(Address, Address),
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepositEvent {
    pub org: Address,
    pub amount: i128,
    pub token: Option<Address>,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawEvent {
    pub org: Address,
    pub recipient: Address,
    pub amount: i128,
    pub token: Option<Address>,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YieldAllocatedEvent {
    pub org: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisbursedEvent {
    pub org: Address,
    pub recipient: Address,
    pub amount: i128,
    pub token: Option<Address>,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InsufficientBalance = 3,
    InvalidAmount = 4,
}

const LEDGER_BUMP: u32 = 120_960; // ~7 days at 5s/ledger
const LEDGER_THRESHOLD: u32 = 100_800; // ~6 days

#[contract]
pub struct TreasuryContract;

#[contractimpl]
impl TreasuryContract {
    /// One-time setup. `admin` is authorized to manage configuration and yield.
    pub fn initialize(
        env: Env,
        admin: Address,
        default_token: Option<Address>,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        if let Some(token) = default_token {
            env.storage().instance().set(&DataKey::DefaultToken, &token);
        }
        env.storage()
            .instance()
            .extend_ttl(LEDGER_THRESHOLD, LEDGER_BUMP);
        Ok(())
    }

    /// Admin updates the default token contract address.
    pub fn set_token(env: Env, token: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        env.storage().instance().set(&DataKey::DefaultToken, &token);
        Ok(())
    }

    pub fn get_token(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::DefaultToken)
    }

    /// Organization deposits funds into its treasury balance.
    /// If default token is configured, transfers tokens from org to contract.
    pub fn deposit(env: Env, org: Address, amount: i128) -> Result<i128, Error> {
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        org.require_auth();

        let default_token: Option<Address> = env.storage().instance().get(&DataKey::DefaultToken);
        if let Some(ref token_addr) = default_token {
            let client = token::Client::new(&env, token_addr);
            client.transfer(&org, env.current_contract_address(), &amount);
        }

        let key = DataKey::Balance(org.clone());
        let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_balance = balance + amount;
        env.storage().persistent().set(&key, &new_balance);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);

        DepositEvent {
            org,
            amount,
            token: default_token,
        }
        .publish(&env);

        Ok(new_balance)
    }

    /// Organization deposits a specific token into its treasury balance.
    pub fn deposit_token(
        env: Env,
        org: Address,
        token: Address,
        amount: i128,
    ) -> Result<i128, Error> {
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        org.require_auth();

        let client = token::Client::new(&env, &token);
        client.transfer(&org, env.current_contract_address(), &amount);

        let key = DataKey::TokenBalance(org.clone(), token.clone());
        let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_balance = balance + amount;
        env.storage().persistent().set(&key, &new_balance);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);

        // Also track aggregate balance
        let agg_key = DataKey::Balance(org.clone());
        let agg_balance: i128 = env.storage().persistent().get(&agg_key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&agg_key, &(agg_balance + amount));

        DepositEvent {
            org,
            amount,
            token: Some(token),
        }
        .publish(&env);

        Ok(new_balance)
    }

    /// Admin releases `amount` from `org`'s balance to `recipient`.
    /// If default token is configured, transfers tokens from contract to recipient.
    pub fn withdraw(
        env: Env,
        org: Address,
        amount: i128,
        recipient: Address,
    ) -> Result<i128, Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let key = DataKey::Balance(org.clone());
        let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        if balance < amount {
            return Err(Error::InsufficientBalance);
        }

        let default_token: Option<Address> = env.storage().instance().get(&DataKey::DefaultToken);
        if let Some(ref token_addr) = default_token {
            let client = token::Client::new(&env, token_addr);
            client.transfer(&env.current_contract_address(), &recipient, &amount);
        }

        let new_balance = balance - amount;
        env.storage().persistent().set(&key, &new_balance);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);

        WithdrawEvent {
            org,
            recipient,
            amount,
            token: default_token,
        }
        .publish(&env);

        Ok(new_balance)
    }

    /// Disburse funds to a verified claimant.
    pub fn disburse(
        env: Env,
        org: Address,
        recipient: Address,
        amount: i128,
        token: Option<Address>,
    ) -> Result<(), Error> {
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let key = DataKey::Balance(org.clone());
        let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        if balance < amount {
            return Err(Error::InsufficientBalance);
        }

        let token_to_use = token.or_else(|| env.storage().instance().get(&DataKey::DefaultToken));
        if let Some(ref token_addr) = token_to_use {
            let client = token::Client::new(&env, token_addr);
            client.transfer(&env.current_contract_address(), &recipient, &amount);
        }

        let new_balance = balance - amount;
        env.storage().persistent().set(&key, &new_balance);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);

        DisbursedEvent {
            org,
            recipient,
            amount,
            token: token_to_use,
        }
        .publish(&env);

        Ok(())
    }

    /// Admin credits `amount` of yield to `org`'s balance.
    pub fn allocate_yield(env: Env, org: Address, amount: i128) -> Result<i128, Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let key = DataKey::Balance(org.clone());
        let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_balance = balance + amount;
        env.storage().persistent().set(&key, &new_balance);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);

        YieldAllocatedEvent { org, amount }.publish(&env);

        Ok(new_balance)
    }

    pub fn balance(env: Env, org: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(org))
            .unwrap_or(0)
    }

    pub fn token_balance(env: Env, org: Address, token: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::TokenBalance(org, token))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup(env: &Env) -> (TreasuryContractClient<'_>, Address, Address) {
        let admin = Address::generate(env);
        let org = Address::generate(env);
        let contract_id = env.register(TreasuryContract, ());
        let client = TreasuryContractClient::new(env, &contract_id);
        client.initialize(&admin, &None);
        (client, admin, org)
    }

    #[test]
    fn double_initialize_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _) = setup(&env);
        let res = client.try_initialize(&admin, &None);
        assert_eq!(res, Err(Ok(Error::AlreadyInitialized)));
    }

    #[test]
    fn deposit_increases_balance() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, org) = setup(&env);

        assert_eq!(client.balance(&org), 0);
        let new_bal = client.deposit(&org, &1_000);
        assert_eq!(new_bal, 1_000);
        assert_eq!(client.balance(&org), 1_000);

        let new_bal2 = client.deposit(&org, &500);
        assert_eq!(new_bal2, 1_500);
        assert_eq!(client.balance(&org), 1_500);
    }

    #[test]
    fn withdraw_decreases_balance() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, org) = setup(&env);
        let recipient = Address::generate(&env);

        client.deposit(&org, &2_000);
        let remaining = client.withdraw(&org, &500, &recipient);
        assert_eq!(remaining, 1_500);
        assert_eq!(client.balance(&org), 1_500);
    }

    #[test]
    fn withdraw_more_than_balance_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, org) = setup(&env);
        let recipient = Address::generate(&env);

        client.deposit(&org, &100);
        let res = client.try_withdraw(&org, &200, &recipient);
        assert_eq!(res, Err(Ok(Error::InsufficientBalance)));
    }

    #[test]
    fn allocate_yield_credits_balance() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, org) = setup(&env);

        client.deposit(&org, &10_000);
        let new_bal = client.allocate_yield(&org, &350);
        assert_eq!(new_bal, 10_350);
        assert_eq!(client.balance(&org), 10_350);
    }

    #[test]
    fn test_sac_token_deposit_and_withdrawal_transitions() {
        let env = Env::default();
        env.mock_all_auths();

        let token_admin = Address::generate(&env);
        let sac_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_address = sac_contract.address();
        let token_client = token::Client::new(&env, &token_address);
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

        let contract_id = env.register(TreasuryContract, ());
        let client = TreasuryContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let org = Address::generate(&env);
        let recipient = Address::generate(&env);

        client.initialize(&admin, &Some(token_address.clone()));

        // Mint initial tokens to org
        token_admin_client.mint(&org, &10_000);
        assert_eq!(token_client.balance(&org), 10_000);
        assert_eq!(token_client.balance(&contract_id), 0);

        // Deposit moves real SAC tokens from org to contract
        let new_bal = client.deposit(&org, &3_000);
        assert_eq!(new_bal, 3_000);
        assert_eq!(token_client.balance(&org), 7_000);
        assert_eq!(token_client.balance(&contract_id), 3_000);
        assert_eq!(client.balance(&org), 3_000);

        // Withdraw moves SAC tokens from contract to recipient
        let rem = client.withdraw(&org, &1_200, &recipient);
        assert_eq!(rem, 1_800);
        assert_eq!(token_client.balance(&contract_id), 1_800);
        assert_eq!(token_client.balance(&recipient), 1_200);

        // Disburse moves SAC tokens from contract to claimant
        let claimant = Address::generate(&env);
        client.disburse(&org, &claimant, &800, &None);
        assert_eq!(token_client.balance(&contract_id), 1_000);
        assert_eq!(token_client.balance(&claimant), 800);
    }
}
