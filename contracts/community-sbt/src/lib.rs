use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, LookupMap, UnorderedSet};
use near_sdk::json_types::U64;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::CryptoHash;
use near_sdk::{env, near_bindgen, require, AccountId, Balance, Gas, PanicOnDefault};

pub use crate::events::*;
pub use crate::interfaces::*;
pub use crate::metadata::*;
pub use crate::storage::*;

mod events;
mod interfaces;
mod metadata;
mod storage;

/// Balance of one mili NEAR, which is 10^23 Yocto NEAR.
pub const MILI_NEAR: Balance = 1_000_000_000_000_000_000_000;
pub const BLACKLIST_COST: Balance = 5 * MILI_NEAR;
pub const GAS_FOR_BLACKLIST: Gas = Gas(6 * Gas::ONE_TERA.0);
/// 1s in nano seconds.
pub const SECOND: u64 = 1_000_000_000;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    // Accounts authorized to issue new SBT
    pub admins: UnorderedSet<AccountId>,
    /// registry of burned accounts.
    pub registry: AccountId,

    pub balances: LookupMap<AccountId, TokenId>,
    pub token_data: LookupMap<TokenId, TokenData>,
    // contract metadata
    pub metadata: LazyOption<SBTContractMetadata>,

    pub next_token_id: TokenId,
    /// time to live in seconds. used for token expiry
    pub ttl: u64,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    /// @admins: initial set of admins
    #[init]
    pub fn new(admins: Vec<AccountId>, metadata: SBTContractMetadata, registry: AccountId) -> Self {
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
            ttl: 3600 * 24 * 365, // ~ 1 year
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
        if let Some(t) = self.token_data.get(&token_id) {
            Some(Token {
                token_id,
                owner_id: t.owner,
                metadata: t.metadata,
            })
        } else {
            None
        }
    }

    /// returns total amount of tokens minted by this contract.
    /// Includes possible expired tokens.
    pub fn sbt_total_supply(&self) -> U64 {
        U64(self.next_token_id - 1)
    }

    /// Query sbt tokens by owner
    /// `from_index` and `limit` are not used - one account can have max one sbt.
    #[allow(unused_variables)]
    pub fn sbt_by_owner(
        &self,
        account: AccountId,
        from_index: Option<U64>,
        limit: Option<u32>,
    ) -> Vec<Token> {
        if let Some(t) = self.balances.get(&account) {
            return vec![Token {
                token_id: t,
                owner_id: account,
                metadata: self.token_data.get(&t).unwrap().metadata,
            }];
        }
        return Vec::new();
    }

    /// returns total supply of non expired SBTs for a given owner.
    pub fn sbt_supply_by_owner(&self, account: AccountId) -> U64 {
        if self.balances.contains_key(&account) {
            1.into()
        } else {
            0.into()
        }
    }

    /**********
     * FUNCTIONS
     **********/

    /// Soulbound transfer implementation.
    /// returns false if caller is not a SBT holder.
    #[payable]
    pub fn sbt_transfer(&mut self, receiver: AccountId) -> bool {
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

        // TODO: add registry (update: burn account and set token) and make a transfer when registry updated succeed
    }

    /**********
     * ADMIN
     **********/

    /// Mints a new SBT for the given receiver.
    /// If `metadata.expires_at` is None then we set it to ` now+self.ttl`.
    /// Panics if `metadata.expires_at > now+self.ttl`.
    pub fn sbt_mint(&mut self, mut metadata: TokenMetadata, receiver: AccountId) {
        self.assert_issuer();
        require!(
            !self.balances.contains_key(&receiver),
            "receiver already has SBT"
        );
        let default_expires_at = env::block_timestamp() / SECOND + self.ttl;
        if let Some(e) = metadata.expires_at {
            require!(
                e <= default_expires_at,
                format!("max metadata.expire_at is {}", default_expires_at)
            );
        } else {
            metadata.expires_at = Some(default_expires_at);
        }

        let token_id = self.next_token_id;
        self.next_token_id += 1;
        self.token_data.insert(
            &token_id,
            &TokenData {
                owner: receiver.clone(),
                metadata,
            },
        );
        self.balances.insert(&receiver, &token_id);
        let event = Events::SbtMint(vec![SbtMintLog {
            owner: receiver.to_string(),
            tokens: vec![token_id],
            memo: None,
        }]);
        emit_event(event);
    }

    /// sbt_renew will update the expire time of provided tokens.
    /// `ttl` is duration seconds to set expire time: `now+ttl`. Panics if ttl > self.ttl.
    pub fn sbt_renew(&mut self, tokens: Vec<TokenId>, ttl: u64, memo: Option<String>) {
        self.assert_issuer();
        require!(
            ttl <= self.ttl,
            format!("ttl must be smaller than {}", self.ttl)
        );
        let expires_at = env::block_timestamp() / SECOND + ttl;
        for t_id in tokens.iter() {
            let mut td = self.token_data.get(&t_id).expect("Token doesn't exist");
            td.metadata.expires_at = Some(expires_at);
            self.token_data.insert(&t_id, &td);
        }
        let events = Events::SbtRenew(SbtRenewLog { tokens, memo });
        emit_event(events);
    }

    /// admin: remove SBT from the given accounts
    pub fn revoke_for(&mut self, accounts: Vec<AccountId>, memo: Option<String>) {
        self.assert_issuer();
        let mut tokens = Vec::with_capacity(accounts.len());
        for a in accounts {
            let t = self.balances.get(&a);
            if let Some(t) = t {
                self.balances.remove(&a);
                self.token_data.remove(&t);
                tokens.push(t);
            }
        }
        let event = Events::SbtRevoke(SbtRevokeLog { tokens, memo });
        emit_event(event);
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
    /// TODO: must be removed for mainnet
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

fn emit_event(event: Events) {
    // Construct the mint log as per the events standard.
    let log: EventLog = EventLog {
        standard: SBT_STANDARD_NAME.to_string(),
        version: METADATA_SPEC.to_string(),
        event,
    };
    env::log_str(&log.to_string());
}

// used to generate a unique prefix in our storage collections (this is to avoid data collisions)
pub(crate) fn hash_account_id(account_id: &AccountId) -> CryptoHash {
    // get the default hasher
    let mut hash = CryptoHash::default();
    // we hash the account ID and return it
    hash.copy_from_slice(&env::sha256(account_id.as_bytes()));
    hash
}
