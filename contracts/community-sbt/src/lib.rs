use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, LookupMap, UnorderedSet};
use near_sdk::json_types::U64;
use near_sdk::{env, near_bindgen, require, AccountId, PanicOnDefault};

use sbt::*;

pub use crate::interfaces::*;
pub use crate::storage::*;

mod interfaces;
mod storage;

/// 1s in nano seconds.
pub const SECOND: u64 = 1_000_000_000;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    // Accounts authorized to issue new SBT
    pub admins: UnorderedSet<AccountId>,
    /// registry of burned accounts.
    pub registry: AccountId,

    // Community SBT has only one class, so only one SBT per account is allowed
    pub balances: LookupMap<AccountId, TokenId>,
    pub token_data: LookupMap<TokenId, TokenData>,
    // contract metadata
    pub metadata: LazyOption<ContractMetadata>,

    pub next_token_id: TokenId,
    /// time to live in ms. Used for token expiry
    pub ttl: u64,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    /// @admins: initial set of admins
    #[init]
    pub fn new(admins: Vec<AccountId>, metadata: ContractMetadata, registry: AccountId) -> Self {
        let mut admin_set = UnorderedSet::new(StorageKey::Admins);
        for a in admins {
            admin_set.insert(&a);
        }
        Self {
            admins: admin_set,
            registry,

            balances: LookupMap::new(StorageKey::Balances),
            token_data: LookupMap::new(StorageKey::TokenData),
            metadata: LazyOption::new(StorageKey::ContractMetadata, Some(&metadata)),
            next_token_id: 1,
            ttl: 1000 * 3600 * 24 * 365, // ~ 1 year in ms
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

    /// returns information about specific token ID
    pub fn sbt(&self, token_id: TokenId) -> Option<Token> {
        self.token_data.get(&token_id).and_then(|t| {
            Some(Token {
                token_id,
                owner_id: t.owner,
                metadata: t.metadata.v1(),
            })
        })
    }

    /// Returns total amount of tokens minted by this contract.
    /// Includes possible expired tokens and revoked tokens.
    // TODO: maybe we will want to use u64 as a return type? But that will break the NFT interface
    // .... nft interface is using U128 anyway
    pub fn nft_total_supply(&self) -> U64 {
        U64(self.sbt_total_supply())
    }

    /// Query sbt tokens by owner
    /// `from_index` and `limit` are not used - one account can have max one sbt.
    // TODO: nft uses U128 instead of U64 ... but it's really not needed.
    #[allow(unused_variables)]
    pub fn nft_tokens_for_owner(
        &self,
        account: AccountId,
        from_index: Option<U64>,
        limit: Option<u64>,
    ) -> Vec<Token> {
        if let Some(t) = self.balances.get(&account) {
            return vec![Token {
                token_id: t,
                owner_id: account,
                metadata: self.token_data.get(&t).unwrap().metadata.v1(),
            }];
        }
        return Vec::new();
    }

    /// alias to sbt_supply_for_owner but returns number as a string instead
    pub fn nft_supply_for_owner(&self, account: AccountId) -> U64 {
        self.sbt_supply_for_owner(account).into()
    }

    // SBT Query version //

    pub fn sbt_total_supply(&self) -> u64 {
        self.next_token_id - 1
    }

    /// returns total supply of non revoked SBTs for a given owner.
    pub fn sbt_supply_for_owner(&self, account: AccountId) -> u64 {
        if self.balances.contains_key(&account) {
            1
        } else {
            0
        }
    }

    /************
     * FUNCTIONS
     ************/

    /// Soulbound transfer implementation.
    /// returns false if caller is not a SBT holder.
    #[allow(unused_variables)]
    #[payable]
    pub fn sbt_transfer(&mut self, receiver: AccountId) -> bool {
        panic!("not implemented");
        /*
                let owner = env::predecessor_account_id();
                if let Some(sbt) = self.balances.get(&owner) {
                    self.balances.remove(&owner);
                    self.balances.insert(&receiver, &sbt);
                    let mut t = self.token_data.get(&sbt).unwrap();
                    t.owner = receiver;
                    self.token_data.insert(&sbt, &t);
                    return true;
                }
                return false;
        */
        // TODO: add registry (update: burn account and set token) and make a transfer when registry updated succeed
    }

    /**********
     * ADMIN
     **********/

    /// Mints a new SBT for the given receiver.
    /// If `metadata.expires_at` is None then we set it to ` now+self.ttl`.
    /// Panics if `metadata.expires_at > now+self.ttl`.
    pub fn sbt_mint(
        &mut self,
        #[allow(unused_mut)] mut metadata: TokenMetadata,
        receiver: AccountId,
    ) {
        self.assert_issuer();
        if str::ends_with(env::current_account_id().as_ref(), "near") {
            require!(str::ends_with(receiver.as_ref(), "near"))
        } else {
            require!(str::ends_with(receiver.as_ref(), "testnet"))
        }

        require!(
            !self.balances.contains_key(&receiver),
            "receiver already has SBT"
        );
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
        let token_id = self.next_token_id;
        self.next_token_id += 1;
        self.token_data.insert(
            &token_id,
            &TokenData {
                owner: receiver.clone(),
                metadata: metadata.into(),
            },
        );
        self.balances.insert(&receiver, &token_id);
        SbtMint {
            owner: &receiver,
            tokens: vec![token_id],
            memo: None,
        }
        .emit();
    }

    /// sbt_renew will update the expire time of provided tokens.
    /// `ttl` is duration seconds to set expire time: `now+ttl`.
    /// Panics if ttl > self.ttl or `tokens` is an empty list.
    pub fn sbt_renew(&mut self, tokens: Vec<TokenId>, ttl: u64, memo: Option<String>) {
        self.assert_issuer();
        require!(
            ttl <= self.ttl,
            format!("ttl must be smaller than {}", self.ttl)
        );
        require!(!tokens.is_empty(), "tokens must be a non empty list");
        let expires_at_ms = env::block_timestamp_ms() + ttl * 1000;
        for t_id in tokens.iter() {
            let mut td = self.token_data.get(&t_id).expect("Token doesn't exist");
            let mut m = td.metadata.v1();
            m.expires_at = Some(expires_at_ms);
            td.metadata = m.into();
            self.token_data.insert(&t_id, &td);
        }
        SbtRenew { tokens, memo }.emit();
    }

    /// admin: remove SBT from the given accounts.
    /// Panics if `accounts` is an empty list.
    pub fn revoke_for(&mut self, accounts: Vec<AccountId>, memo: Option<String>) {
        self.assert_issuer();
        require!(!accounts.is_empty(), "accounts must be a non empty list");
        let mut tokens = Vec::with_capacity(accounts.len());
        for a in accounts {
            match self.balances.get(&a) {
                Some(t) => {
                    self.balances.remove(&a);
                    self.token_data.remove(&t);
                    tokens.push(t);
                }
                _ => (),
            }
        }
        if !tokens.is_empty() {
            SbtRevoke { tokens, memo }.emit();
        }
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

    // /// Testing function
    // /// @`ttl`: expire time to live in seconds
    // pub fn admin_change_ttl(&mut self, ttl: u64) {
    //     self.assert_issuer();
    //     self.ttl = ttl;
    // }

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
impl SBTMetadata for Contract {
    fn sbt_metadata(&self) -> ContractMetadata {
        self.metadata.get().unwrap()
    }
}
