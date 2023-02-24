use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, UnorderedMap, UnorderedSet, Vector};
use near_sdk::{env, near_bindgen, AccountId, CryptoHash, PanicOnDefault};

use sbt::TokenId;

use crate::events::*;
use crate::storage::*;

mod events;
mod storage;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// registry of blacklisted accounts by issuer
    pub blacklist: UnorderedMap<AccountId, UnorderedSet<AccountId>>,

    /// maps user account to list of tokens source info
    pub tokens: LookupMap<AccountId, Vector<TokenSrc>>,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    #[init]
    pub fn new() -> Self {
        Self {
            blacklist: UnorderedMap::new(StorageKey::Blacklist),
            tokens: LookupMap::new(StorageKey::Tokens),
        }
    }

    /// Called on SBT mint. Will append token id to the registry.
    #[allow(unused_variables)]
    pub fn on_token_mint(&mut self, ctr: AccountId, receipient: AccountId, token: TokenId) {
        let ctr = env::predecessor_account_id();
        let mut _s = self.tokens.get(&ctr);
        // TODO: use proper composed key
        //.unwrap_or_else(|| Vector::new(receipient.as_bytes()));
    }

    /// Permission less function -- anyone can call it to claim an account to be blacklisted.
    /// However, we should recoginze a set of legit contracts who will blacklist accounts to make
    /// sense of it.
    /// SBT contracts should blacklist accounts during a recovery process.
    pub fn blacklist(&mut self, account: AccountId, memo: Option<String>) {
        // TODO: add storage fees

        let ctr = env::predecessor_account_id();
        let mut _s = self.blacklist.get(&ctr);
        // TODO: use proper composed key
        // .unwrap_or_else(|| UnorderedSet::new(ctr.as_bytes()));
        // s.insert(account);
        // self.blacklist.insert(ctr, s);

        let event = BlacklistLog {
            caller: ctr,
            account,
            memo,
        };
        emit_event(event);
    }

    /// checks if an `account` was blacklisted by `ctr` contract in an event
    /// of SBT (soulbound token) recovery process.
    pub fn is_blacklisted(&self, ctr: AccountId, account: AccountId) -> bool {
        self.blacklist
            .get(&ctr)
            .map_or(false, |s| s.contains(&account))
    }
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct TokenSrc {
    id: TokenId,
    /// SBT contract address
    contract: AccountId,
}

// used to generate a unique prefix in our storage collections (this is to avoid data collisions)
pub(crate) fn hash_account_id(account_id: &AccountId) -> CryptoHash {
    // get the default hasher
    let mut hash = CryptoHash::default();
    // we hash the account ID and return it
    hash.copy_from_slice(&env::sha256(account_id.as_bytes()));
    hash
}
