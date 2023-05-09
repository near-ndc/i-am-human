use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, UnorderedSet};
use near_sdk::{env, near_bindgen, require, AccountId, Gas, PanicOnDefault, Promise};

use cost::{MINT_COST, MINT_GAS};
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
    /// time to live in ms. Used for token expiry
    pub ttl: u64,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    /// @admins: initial set of admins
    /// @ttl: default (if expire_at is set in sbt_mint metadata) and maximum (if expire_at is
    ///    not  set in metadata) time to live for SBT expire. Must be number in miliseconds.
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
        return self.admins.contains(&addr);
    }

    /**********
     * ADMIN
     **********/

    /// Mints a new SBT for the given receiver.
    /// If `metadata.expires_at` is None then we set it to max: ` now+self.ttl`.
    /// Panics if `metadata.expires_at > now+self.ttl`.
    #[payable]
    pub fn sbt_mint(
        &mut self,
        receiver: AccountId,
        #[allow(unused_mut)] mut metadata: TokenMetadata,
        memo: Option<String>,
    ) -> Promise {
        require!(
            env::attached_deposit() == MINT_COST,
            "Requires attached deposit of exactly 0.007 NEAR"
        );

        self.assert_issuer();
        if str::ends_with(env::current_account_id().as_ref(), "near") {
            require!(str::ends_with(receiver.as_ref(), "near"))
        } else {
            require!(str::ends_with(receiver.as_ref(), "testnet"))
        }

        let now_ms = env::block_timestamp_ms();
        let default_expires_at = now_ms + self.ttl;
        if let Some(e) = metadata.expires_at {
            require!(
                e <= default_expires_at,
                format!("max metadata.expire_at is {}", default_expires_at)
            );
        } else {
            metadata.expires_at = Some(default_expires_at);
        }
        metadata.issued_at = Some(now_ms);
        if metadata.class == 0 {
            metadata.class = 1;
        }

        if let Some(memo) = memo {
            env::log_str(&format!("SBT mint memo: {}", memo));
        }
        ext_registry::ext(self.registry.clone())
            .with_attached_deposit(MINT_COST) // no extra cost needed
            .with_static_gas(MINT_GAS)
            .sbt_mint(vec![(receiver, vec![metadata])])
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(Gas::ONE_TERA * 3)
                    .sbt_mint_callback(),
            )
    }

    #[private]
    pub fn sbt_mint_callback(
        &mut self,
        #[callback_result] last_result: Result<Vec<TokenId>, near_sdk::PromiseError>,
    ) {
        match last_result {
            Ok(v) => (),
            Err(_) => panic!("registry mint failed"),
        }
    }

    /// sbt_renew will update the expire time of provided tokens.
    /// `ttl` is duration in seconds to set expire time: `now+ttl`.
    /// Panics if ttl > self.ttl or `tokens` is an empty list.
    pub fn sbt_renew(&mut self, tokens: Vec<TokenId>, ttl: u64, memo: Option<String>) {
        self.assert_issuer();
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
    pub fn revoke_for(&mut self, accounts: Vec<AccountId>, memo: Option<String>) {
        self.assert_issuer();
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

    pub fn add_admins(&mut self, admins: Vec<AccountId>) {
        self.assert_issuer();
        for a in admins {
            self.admins.insert(&a);
        }
    }

    /// Any admin can remove any other admin.
    // TODO: probably we should change this.
    pub fn remove_admins(&mut self, admins: Vec<AccountId>) {
        self.assert_issuer();
        for a in admins {
            self.admins.remove(&a);
        }
    }

    /// Testing function
    /// @`ttl`: expire time to live in seconds
    pub fn admin_change_ttl(&mut self, ttl: u64) {
        self.assert_issuer();
        self.ttl = ttl;
    }

    /**********
     * INTERNAL
     **********/

    fn assert_issuer(&self) {
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
