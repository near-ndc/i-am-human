use std::collections::HashSet;
use std::mem::size_of;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, LookupMap, UnorderedMap, UnorderedSet};
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::CryptoHash;
use near_sdk::{
    assert_one_yocto, env, ext_contract, log, near_bindgen, require, AccountId, Balance,
    PanicOnDefault, Promise, PromiseOrValue, PromiseResult, ONE_YOCTO,
};

pub use crate::events::*;
pub use crate::metadata::*;
pub use crate::storage::*;

mod events;
mod metadata;
mod storage;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    // Address of an account authorized to issue new SBT tokens
    pub issuer: AccountId,
    // Address of an account authorized to renew or recover SBT tokens
    pub operators: HashSet<AccountId>,

    // keeps track of all the token IDs for a given account
    pub tokens_per_owner: LookupMap<AccountId, UnorderedSet<TokenId>>,
    // keeps track of the token metadata for a given token ID
    pub token_metadata: UnorderedMap<TokenId, TokenMetadata>,
    // keeps track of the metadata for the contract
    pub metadata: LazyOption<SBTContractMetadata>,

    pub next_token_id: TokenId,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(
        issuer: AccountId,
        operators: Vec<AccountId>,
        metadata: SBTContractMetadata,
    ) -> Self {
        Self {
            issuer,
            operators: HashSet::from_iter(operators),

            tokens_per_owner: LookupMap::new(StorageKey::TokensPerOwner.try_to_vec().unwrap()),
            token_metadata: UnorderedMap::new(StorageKey::TokenMetadataById.try_to_vec().unwrap()),
            metadata: LazyOption::new(
                StorageKey::SBTContractMetadata.try_to_vec().unwrap(),
                Some(&metadata),
            ),
            next_token_id: 1,
        }
    }

    #[payable]
    pub fn sbt_mint(&mut self, metadata: TokenMetadata, receiver: AccountId) {
        self.assert_issuer();
        assert_one_yocto();

        let token_id = self.next_token_id;
        self.next_token_id += 1;
        self.token_metadata.insert(&token_id, &metadata);
        self.add_token_to_owner(&receiver, &token_id);

        // Construct the mint log as per the events standard.
        let sbt_mint_log: EventLog = EventLog {
            standard: SBT_STANDARD_NAME.to_string(),
            version: METADATA_SPEC.to_string(),
            event: EventLogVariant::SbtMint(vec![SbtMintLog {
                owner: receiver.to_string(),
                tokens: vec![token_id.to_string()],
                memo: None,
            }]),
        };
        env::log_str(&sbt_mint_log.to_string());
    }

    /**********
     * INTERNAL
     **********/

    fn assert_issuer(&self) {
        assert_eq!(self.issuer, env::predecessor_account_id(), "must be issuer");
    }

    fn assert_operator(&self) {
        assert!(
            self.operators.contains(&env::predecessor_account_id()),
            "must be operator"
        );
    }

    /// add a token to the set of tokens an owner has
    pub(crate) fn add_token_to_owner(&mut self, account_id: &AccountId, token_id: &TokenId) {
        let mut tokens_set = self.tokens_per_owner.get(account_id).unwrap_or_else(|| {
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

        tokens_set.insert(token_id);
        self.tokens_per_owner.insert(account_id, &tokens_set);
    }

    pub(crate) fn remove_token_from_owner(&mut self, owner: &AccountId, token_id: &TokenId) {
        // we get the set of tokens that the owner has
        let mut tokens_set = self
            .tokens_per_owner
            .get(owner)
            .expect("Token not owned by the owner");

        // we remove the the token_id from the set of tokens
        tokens_set.remove(token_id);

        // if the token set is now empty, we remove the owner from the tokens_per_owner collection
        if tokens_set.is_empty() {
            self.tokens_per_owner.remove(owner);
        } else {
            // otherwise, we simply insert it back for the account ID.
            self.tokens_per_owner.insert(owner, &tokens_set);
        }
    }
}

//used to generate a unique prefix in our storage collections (this is to avoid data collisions)
pub(crate) fn hash_account_id(account_id: &AccountId) -> CryptoHash {
    //get the default hash
    let mut hash = CryptoHash::default();
    //we hash the account ID and return it
    hash.copy_from_slice(&env::sha256(account_id.as_bytes()));
    hash
}
