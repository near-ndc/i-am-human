use std::collections::HashSet;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, LookupMap, UnorderedMap, UnorderedSet};
use near_sdk::json_types::U64;
use near_sdk::CryptoHash;
use near_sdk::{
    assert_one_yocto, env, near_bindgen, require, AccountId, Balance, Gas, PanicOnDefault,
};

use sbt::*;

pub use crate::interfaces::*;
pub use crate::storage::*;

mod interfaces;
mod storage;

/// Balance of one mili NEAR, which is 10^23 Yocto NEAR.
pub const MILI_NEAR: Balance = 1_000_000_000_000_000_000_000;
pub const BLACKLIST_COST: Balance = 5 * MILI_NEAR;
pub const GAS_FOR_BLACKLIST: Gas = Gas(6 * Gas::ONE_TERA.0);

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    // Address of an account authorized to issue new SBT tokens
    pub issuer: AccountId,
    // Address of an account authorized to renew or recover SBT tokens
    pub operators: HashSet<AccountId>,
    pub blacklist_registry: AccountId,

    pub token_to_owner: UnorderedMap<TokenId, AccountId>,
    // keeps track of all the token IDs for a given account
    pub balances: LookupMap<AccountId, UnorderedSet<TokenId>>,
    // token metadata
    pub token_metadata: UnorderedMap<TokenId, TokenMetadata>,
    // contract metadata
    pub metadata: LazyOption<ContractMetadata>,

    pub next_token_id: TokenId,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(
        issuer: AccountId,
        operators: Vec<AccountId>,
        metadata: ContractMetadata,
        blacklist_registry: AccountId,
    ) -> Self {
        Self {
            issuer,
            operators: HashSet::from_iter(operators),
            blacklist_registry,

            token_to_owner: UnorderedMap::new(StorageKey::TokenToOwner.try_to_vec().unwrap()),
            balances: LookupMap::new(StorageKey::TokensPerOwner.try_to_vec().unwrap()),
            token_metadata: UnorderedMap::new(StorageKey::TokenMetadataById.try_to_vec().unwrap()),
            metadata: LazyOption::new(
                StorageKey::SBTContractMetadata.try_to_vec().unwrap(),
                Some(&metadata),
            ),
            next_token_id: 1,
        }
    }

    /**********
     * QUERIES
     **********/

    // get the information about specific token ID
    pub fn sbt(&self, token_id: TokenId) -> Option<Token> {
        if let Some(metadata) = self.token_metadata.get(&token_id) {
            Some(Token {
                token_id,
                owner_id: self.token_to_owner.get(&token_id).unwrap(),
                metadata,
            })
        } else {
            None
        }
    }

    // returns total amount of tokens minted by this contract
    pub fn sbt_total_supply(&self) -> U64 {
        U64(self.next_token_id - 1)
    }

    // returns total supply of SBTs for a given owner
    pub fn sbt_supply_by_owner(&self, account: AccountId) -> U64 {
        //get the set of tokens for the passed in owner
        let tokens_for_owner_set = self.balances.get(&account);

        //if there is some set of tokens, we'll return the length as a U128
        if let Some(tokens_for_owner_set) = tokens_for_owner_set {
            U64(tokens_for_owner_set.len() as u64)
        } else {
            U64(0)
        }
    }

    // Query for sbt tokens
    pub fn sbt_tokens(&self, from_index: Option<U64>, limit: Option<u32>) -> Vec<Token> {
        //where to start pagination - if we have a from_index, we'll use that - otherwise start from 0 index
        let start = u64::from(from_index.unwrap_or(U64(0)));

        self.token_metadata
            .keys()
            .skip(start as usize)
            .take(limit.unwrap_or(50) as usize)
            .map(|t| self.sbt(t.clone()).unwrap())
            .collect()
    }

    // Query sbt tokens by owner
    pub fn sbt_tokens_by_owner(
        &self,
        account: AccountId,
        from_index: Option<U64>,
        limit: Option<u32>,
    ) -> Vec<Token> {
        let tokens_for_owner_set = self.balances.get(&account);
        let tokens = if let Some(tokens_for_owner_set) = tokens_for_owner_set {
            tokens_for_owner_set
        } else {
            // if there is no set of tokens, we'll simply return an empty vector.
            return vec![];
        };

        let start = u64::from(from_index.unwrap_or(U64(0)));
        tokens
            .iter()
            .skip(start as usize)
            .take(limit.unwrap_or(50) as usize)
            .map(|t| self.sbt(t.clone()).unwrap())
            .collect()
    }

    // Optional function to implement for SBT with max supply = 1 per account.
    // /// returns total supply of SBTs for a given owner
    // pub fn sbt_token_by_owner(&self, account: AccountId) -> Option<Token> {}

    /**********
     * ADMIN
     **********/

    #[payable]
    pub fn sbt_mint(&mut self, metadata: TokenMetadata, receiver: AccountId) {
        self.assert_issuer();
        assert_one_yocto();

        let token_id = self.next_token_id;
        self.next_token_id += 1;
        self.token_metadata.insert(&token_id, &metadata);
        self.add_token_to_owner(&receiver, token_id);
        let event = Nep393EventKind::SbtMint(vec![SbtMint {
            owner: receiver.to_string(),
            tokens: vec![token_id],
            memo: None,
        }]);
        emit_event(event);
    }

    /// sbt_recover reassigns all tokens from the old_owner to the new_owner,
    /// and registers `old_owner` to a burned addresses registry.
    /// Must be called by operator.
    /// Must provide 5 miliNEAR to cover registry storage cost. Operator should
    ///   put that cost to the requester (old_owner), eg by asking operation fee.
    #[payable]
    pub fn sbt_recover(&mut self, from: AccountId, to: AccountId) {
        self.assert_operator();
        require!(
            env::attached_deposit() >= BLACKLIST_COST,
            "must provide at least 5 miliNEAR to cover blacklist storage cost"
        );

        let token_set_old = self
            .balances
            .get(&from)
            .expect("Token not owned by the owner");

        // we remove from records, and merge his tokens into to token set
        self.balances.remove(&from);
        let mut token_set_new = self.balances.get(&to).unwrap_or_else(|| {
            UnorderedSet::new(
                StorageKey::TokenPerOwnerInner {
                    //we get a new unique prefix for the collection
                    account_id_hash: hash_account_id(&to),
                }
                .try_to_vec()
                .unwrap(),
            )
        });
        for t in token_set_old.iter() {
            token_set_new.insert(&t);
            self.token_to_owner.insert(&t, &to);
        }
        self.balances.insert(&to, &token_set_new);

        let event = Nep393EventKind::SbtRecover(vec![SbtRecover {
            old_owner: from.to_string(),
            new_owner: to.to_string(),
            tokens: token_set_old.iter().collect(),
            memo: None,
        }]);
        emit_event(event);

        ext_blacklist::ext(self.blacklist_registry.clone())
            .with_attached_deposit(BLACKLIST_COST)
            .with_static_gas(GAS_FOR_BLACKLIST)
            .blacklist(from, None);
    }

    /// sbt_renew will update the expire time of provided tokens.
    /// `expires_at` is a unix timestamp (in seconds).
    #[payable]
    pub fn sbt_renew(&mut self, tokens: Vec<TokenId>, expires_at: u64, memo: Option<String>) {
        self.assert_operator();
        assert_one_yocto();

        for t_id in tokens.iter() {
            let mut t = self.token_metadata.get(&t_id).expect("Token doesn't exist");
            t.expires_at = Some(expires_at);
            self.token_metadata.insert(&t_id, &t);
        }

        let event = Nep393EventKind::SbtRenew(SbtRenew { tokens, memo });
        emit_event(event);
    }

    /**********
     * INTERNAL
     **********/

    fn assert_issuer(&self) {
        require!(
            self.issuer == env::predecessor_account_id(),
            "must be issuer"
        );
    }

    fn assert_operator(&self) {
        require!(
            self.operators.contains(&env::predecessor_account_id()),
            "must be operator"
        );
    }

    /// add a token to the set of tokens an owner has
    pub(crate) fn add_token_to_owner(&mut self, account_id: &AccountId, token_id: TokenId) {
        let mut tokens_set = self.balances.get(account_id).unwrap_or_else(|| {
            //if the account doesn't have any tokens, we create a new unordered set
            UnorderedSet::new(
                StorageKey::TokenPerOwnerInner {
                    //we get a new unique prefix for the collection
                    account_id_hash: hash_account_id(&account_id),
                }
                .try_to_vec()
                .unwrap(),
            )
        });

        tokens_set.insert(&token_id);
        self.balances.insert(account_id, &tokens_set);
        self.token_to_owner.insert(&token_id, account_id);
    }
}

// used to generate a unique prefix in our storage collections (this is to avoid data collisions)
pub(crate) fn hash_account_id(account_id: &AccountId) -> CryptoHash {
    // get the default hasher
    let mut hash = CryptoHash::default();
    // we hash the account ID and return it
    hash.copy_from_slice(&env::sha256(account_id.as_bytes()));
    hash
}
