use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{AccountId, BorshStorageKey};
use sbt::{ClassId, TokenId};

/// Issuer contract ID based on the SBT Contract address -> u16 map.
pub type CtrId = u16;

/// Helper structure for keys of the persistent collections.
#[derive(BorshSerialize, BorshStorageKey)]
pub enum StorageKey {
    SbtContracts,
    SbtContractsRev,
    Banlist,
    Balances,
    BalancesMap { owner: AccountId },
    CtrTokens,
    NextTokenId,
}

/// contract token id used for collection indexing
#[derive(BorshSerialize, BorshDeserialize)]
pub(crate) struct CtrTokenId {
    pub ctr_id: CtrId,
    pub token: TokenId,
}

// TODO: remove debug
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub(crate) struct CtrClassId {
    pub ctr_id: CtrId,
    pub class_id: ClassId,
}

// TODO: storage check
//
// use near_sdk::CryptoHash;
//
// #[derive(BorshSerialize)]
// pub enum StorageKey {
//     TokenPerOwnerInner { account_id_hash: CryptoHash },
// }
// StorageKey::TokenPerOwnerInner {
//     //we get a new unique prefix for the collection
//     account_id_hash: hash_account_id(&to),
// }
// .try_to_vec()
// .unwrap(),

/*
// used to generate a unique prefix in our storage collections (this is to avoid data collisions)
pub(crate) fn hash_account_id(account_id: &AccountId) -> CryptoHash {
    // get the default hasher
    let mut hash = CryptoHash::default();
    // we hash the account ID and return it
    hash.copy_from_slice(&env::sha256(account_id.as_bytes()));
    hash
}

 */
