use std::collections::HashSet;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, LookupMap, UnorderedMap, UnorderedSet};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::CryptoHash;
use near_sdk::{
    assert_one_yocto, env, ext_contract, log, near_bindgen, require, AccountId, PanicOnDefault,
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
        let event = EventLogVariant::SbtMint(vec![SbtMintLog {
            owner: receiver.to_string(),
            tokens: vec![token_id],
            memo: None,
        }]);
        emit_event(event);
    }

    /// sbt_recover reassigns all tokens from the old_owner to the new_owner,
    /// and registers `old_owner` to a burned addresses registry.
    #[payable]
    pub fn sbt_recover(&mut self, old_owner: AccountId, new_owner: AccountId) {
        self.assert_operator();
        assert_one_yocto();

        let token_set_old = self
            .tokens_per_owner
            .get(&old_owner)
            .expect("Token not owned by the owner");

        // we remove old_owner records, and merge his tokens into new_owner token set
        self.tokens_per_owner.remove(&old_owner);
        let mut token_set_new = self.tokens_per_owner.get(&new_owner).unwrap_or_else(|| {
            UnorderedSet::new(
                StorageKey::TokenPerOwnerInner {
                    //we get a new unique prefix for the collection
                    account_id_hash: hash_account_id(&new_owner),
                }
                .try_to_vec()
                .unwrap(),
            )
        });
        for t in token_set_old.iter() {
            token_set_new.insert(&t);
        }
        self.tokens_per_owner.insert(&new_owner, &token_set_new);

        // TODO: register the old_account into burned sbt account set contract

        let event = EventLogVariant::SbtRecover(vec![SbtRecoverLog {
            old_owner: old_owner.to_string(),
            new_owner: new_owner.to_string(),
            tokens: token_set_old.iter().collect(),
            memo: None,
        }]);
        emit_event(event);
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

        let event = EventLogVariant::SbtRenew(vec![SbtRenewLog { tokens, memo }]);
        emit_event(event);
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
    pub(crate) fn add_token_to_owner(&mut self, account_id: &AccountId, token_id: TokenId) {
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

        tokens_set.insert(&token_id);
        self.tokens_per_owner.insert(account_id, &tokens_set);
    }
}

fn emit_event(event: EventLogVariant) {
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
