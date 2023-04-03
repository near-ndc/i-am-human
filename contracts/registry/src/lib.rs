use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, UnorderedSet, Vector};
use near_sdk::{env, near_bindgen, require, AccountId, CryptoHash, PanicOnDefault};

use sbt::TokenId;

use crate::storage::*;

mod registry;
mod storage;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    pub admin: AccountId,

    /// registry of blacklisted accounts by issuer
    pub sbt_issuers: UnorderedSet<AccountId>,

    /// registry of blacklisted accounts by issuer
    pub banlist: UnorderedSet<AccountId>,

    /// maps user account to list of tokens source info
    pub tokens: LookupMap<AccountId, Vector<TokenSrc>>,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(admin: AccountId) -> Self {
        Self {
            admin,
            sbt_issuers: UnorderedSet::new(StorageKey::Issuers),
            banlist: UnorderedSet::new(StorageKey::Banlist),
            tokens: LookupMap::new(StorageKey::Tokens),
        }
    }

    /// returns false if the `issuer` contract was already registered.
    pub fn add_sbt_issuer(&mut self, issuer: AccountId) -> bool {
        // TODO: add admin check
        self.sbt_issuers.insert(&issuer)
    }

    //
    // Internal
    //

    pub(crate) fn assert_issuer(&self, contract: &AccountId) {
        require!(self.sbt_issuers.contains(contract))
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
