use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, UnorderedSet};
use near_sdk::{env, near_bindgen, require, AccountId, Balance, PanicOnDefault, Promise};

use cost::{MILI_NEAR, MINT_COST, MINT_GAS};
use sbt::*;

pub use crate::storage::*;
mod storage;

/// 1s in nano seconds.
pub const SECOND: u64 = 1_000_000_000;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// Accounts authorized to issue new SBT
    pub admins: UnorderedSet<AccountId>,
    /// SBT registry.
    pub registry: AccountId,

    /// contract metadata
    pub metadata: LazyOption<ContractMetadata>,
    /// time to live in ms. Overwrites metadata.expire_at.
    pub ttl: u64,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    /// @admins: initial set of admins
    /// @ttl: time to live for SBT expire. Must be number in miliseconds.
    #[init]
    pub fn new(
        registry: AccountId,
        admins: Vec<AccountId>,
        metadata: ContractMetadata,
        ttl: u64,
    ) -> Self {
        let mut admin_set = UnorderedSet::new(StorageKey::Admins);
        for a in admins {
            admin_set.insert(&a);
        }
        require!(ttl > 0, "`ttl` must be bigger than 0");
        Self {
            admins: admin_set,
            registry,

            metadata: LazyOption::new(StorageKey::ContractMetadata, Some(&metadata)),
            ttl,
        }
    }

    /**********
     * QUERIES
     **********/

    /// returns true if given address, or caller (if account is None)
    /// is an admin.
    pub fn is_admin(&self, addr: AccountId) -> bool {
        self.admins.contains(&addr)
    }

    /**********
     * ADMIN
     **********/

    /// Mints a new SBT for the given receiver.
    /// If `metadata.expires_at` is None then we set it to max: ` now+self.ttl`.
    /// Panics if `metadata.expires_at > now+self.ttl` or when ClassID is not set or not 1.
    #[payable]
    pub fn sbt_mint(
        &mut self,
        receiver: AccountId,
        #[allow(unused_mut)] mut metadata: TokenMetadata,
        memo: Option<String>,
    ) -> Promise {
        let required_deposit = required_sbt_mint_deposit(1);
        let attached_deposit = env::attached_deposit();
        require!(
            attached_deposit >= required_deposit,
            format!("deposit must be at least {}N", required_deposit)
        );
        self.assert_admin();

        let now_ms = env::block_timestamp_ms();
        metadata.expires_at = Some(now_ms + self.ttl);
        metadata.issued_at = Some(now_ms);
        require!(metadata.class == 1, "class ID must be 1");

        if let Some(memo) = memo {
            env::log_str(&format!("SBT mint memo: {}", memo));
        }

        ext_registry::ext(self.registry.clone())
            .with_attached_deposit(attached_deposit)
            .with_static_gas(MINT_GAS)
            .sbt_mint(vec![(receiver, vec![metadata])])
    }

    /// sbt_renew will update the expire time of provided tokens.
    /// `ttl` is duration in milliseconds to set expire time: `now+ttl`.
    /// Panics if ttl > self.ttl or `tokens` is an empty list.
    pub fn sbt_renew(&mut self, tokens: Vec<TokenId>, ttl: u64, memo: Option<String>) {
        self.assert_admin();
        require!(
            ttl <= self.ttl,
            format!("ttl must be smaller than {}", self.ttl)
        );
        require!(!tokens.is_empty(), "tokens must be a non empty list");
        let expires_at_ms = env::block_timestamp_ms() + ttl * 1000;
        ext_registry::ext(self.registry.clone()).sbt_renew(tokens, expires_at_ms);

        if let Some(memo) = memo {
            env::log_str(&format!("SBT renew memo: {}", memo));
        }
    }

    /// admin: remove SBT from the given accounts.
    /// Panics if `accounts` is an empty list.
    pub fn revoke_for(
        &mut self,
        accounts: Vec<AccountId>,
        #[allow(unused_variables)] memo: Option<String>,
    ) {
        self.assert_admin();
        require!(!accounts.is_empty(), "accounts must be a non empty list");
        panic!("not implemented");
        // let mut tokens = Vec::with_capacity(accounts.len());
        // for a in accounts {
        //     tokens.push(t);
        // }
        // if !tokens.is_empty() {
        //     SbtTokensEvent { tokens, memo }.emit_revoke();
        // }
    }

    /**********
     * INTERNAL
     **********/

    fn assert_admin(&self) {
        require!(
            self.admins.contains(&env::predecessor_account_id()),
            "must be issuer"
        );
    }
}

#[near_bindgen]
impl SBTContract for Contract {
    fn sbt_metadata(&self) -> ContractMetadata {
        self.metadata.get().unwrap()
    }
}

#[inline]
pub fn required_sbt_mint_deposit(num_tokens: usize) -> Balance {
    (num_tokens as u128) * MINT_COST + MILI_NEAR
}

#[cfg(test)]
mod tests {
    use cost::MILI_NEAR;
    use near_sdk::{test_utils::VMContextBuilder, testing_env, AccountId, Balance, VMContext};
    use sbt::ContractMetadata;

    use crate::Contract;

    const START: u64 = 10;
    const MINT_DEPOSIT: Balance = 6 * MILI_NEAR;

    fn alice() -> AccountId {
        AccountId::new_unchecked("alice.near".to_string())
    }

    fn admin() -> AccountId {
        AccountId::new_unchecked("sbt.near".to_string())
    }

    fn registry() -> AccountId {
        AccountId::new_unchecked("registry.near".to_string())
    }

    fn contract_metadata() -> ContractMetadata {
        return ContractMetadata {
            spec: "community-sbt-0.0.1".to_string(),
            name: "community-sbt".to_string(),
            symbol: "COMMUNITY_SBT".to_string(),
            icon: None,
            base_uri: None,
            reference: None,
            reference_hash: None,
        };
    }

    fn setup(predecessor: &AccountId, deposit: Balance) -> (VMContext, Contract) {
        let mut ctx = VMContextBuilder::new()
            .predecessor_account_id(admin())
            .block_timestamp(START)
            .is_view(false)
            .build();
        if deposit > 0 {
            ctx.attached_deposit = deposit
        }
        testing_env!(ctx.clone());
        let ctr = Contract::new(registry(), vec![admin()], contract_metadata(), START);
        ctx.predecessor_account_id = predecessor.clone();
        testing_env!(ctx.clone());
        return (ctx, ctr);
    }

    #[test]
    fn is_admin() {
        let (_, ctr) = setup(&alice(), MINT_DEPOSIT);
        assert!(ctr.is_admin(admin()));
        assert!(!ctr.is_admin(alice()));
    }
}
