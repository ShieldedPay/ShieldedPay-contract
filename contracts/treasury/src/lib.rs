//! ShieldedPay Treasury Contract
//!
//! Holds each organization's payroll funds and tracks per-organization
//! balances on-chain. `deposit`/`withdraw` move an org's ledger balance;
//! `allocate_yield` credits yield earned off-chain (e.g. via a BENJI-style
//! strategy) back to an org's balance.
//!
//! Actual token transfers (moving real XLM/asset balances in and out of this
//! contract) are intentionally out of scope here and tracked separately --
//! see ISSUE-BACKLOG.md. This contract owns the authoritative ledger of who
//! is owed what; wiring it to a real token client is the next step.

#![no_std]
// TODO: migrate to the #[contractevent] macro (see ISSUE-BACKLOG.md) --
// events().publish() still works but is deprecated in soroban-sdk 27.x.
#![allow(deprecated)]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env};

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Balance(Address),
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
    /// One-time setup. `admin` is the only address allowed to withdraw or
    /// allocate yield on behalf of an organization.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .extend_ttl(LEDGER_THRESHOLD, LEDGER_BUMP);
        Ok(())
    }

    /// Organization deposits funds into its treasury balance. The org must
    /// authorize the call. `amount` is denominated in the smallest unit of
    /// whatever asset the deployment uses.
    pub fn deposit(env: Env, org: Address, amount: i128) -> Result<i128, Error> {
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        org.require_auth();

        let key = DataKey::Balance(org.clone());
        let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_balance = balance + amount;
        env.storage().persistent().set(&key, &new_balance);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);

        env.events()
            .publish((soroban_sdk::symbol_short!("deposit"), org), amount);

        Ok(new_balance)
    }

    /// Admin releases `amount` from `org`'s balance to `recipient` (e.g. the
    /// payroll contract, ahead of a disbursement round).
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
        let new_balance = balance - amount;
        env.storage().persistent().set(&key, &new_balance);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);

        env.events().publish(
            (soroban_sdk::symbol_short!("withdraw"), org, recipient),
            amount,
        );

        Ok(new_balance)
    }

    /// Admin credits `amount` of yield to `org`'s balance. The actual yield
    /// figure is computed off-chain by whatever strategy is running; this
    /// call only records the credit.
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

        env.events()
            .publish((soroban_sdk::symbol_short!("yield"), org), amount);

        Ok(new_balance)
    }

    pub fn balance(env: Env, org: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(org))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup(env: &Env) -> (TreasuryContractClient, Address, Address) {
        let admin = Address::generate(env);
        let org = Address::generate(env);
        let contract_id = env.register(TreasuryContract, ());
        let client = TreasuryContractClient::new(env, &contract_id);
        client.initialize(&admin);
        (client, admin, org)
    }

    #[test]
    fn deposit_increases_balance() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, org) = setup(&env);

        assert_eq!(client.deposit(&org, &1000), 1000);
        assert_eq!(client.balance(&org), 1000);

        assert_eq!(client.deposit(&org, &500), 1500);
        assert_eq!(client.balance(&org), 1500);
    }

    #[test]
    fn withdraw_decreases_balance() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, org) = setup(&env);
        let recipient = Address::generate(&env);

        client.deposit(&org, &1000);
        assert_eq!(client.withdraw(&org, &400, &recipient), 600);
        assert_eq!(client.balance(&org), 600);
    }

    #[test]
    fn withdraw_more_than_balance_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, org) = setup(&env);
        let recipient = Address::generate(&env);

        client.deposit(&org, &100);
        let result = client.try_withdraw(&org, &500, &recipient);
        assert_eq!(result, Err(Ok(Error::InsufficientBalance)));
    }

    #[test]
    fn allocate_yield_credits_balance() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, org) = setup(&env);

        client.deposit(&org, &1000);
        assert_eq!(client.allocate_yield(&org, &50), 1050);
    }

    #[test]
    fn double_initialize_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(TreasuryContract, ());
        let client = TreasuryContractClient::new(&env, &contract_id);

        client.initialize(&admin);
        let result = client.try_initialize(&admin);
        assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
    }
}
