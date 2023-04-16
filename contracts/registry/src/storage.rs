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
    SupplyByOwner,
    SupplyByClass,
    SupplyByCtr,
    Balances,
    CtrTokens,
    NextTokenId,
    OngoingSoultTx,
}

/// contract token id used for collection indexing
#[derive(BorshSerialize, BorshDeserialize)]
pub(crate) struct CtrTokenId {
    pub ctr_id: CtrId,
    pub token: TokenId,
}

#[derive(BorshSerialize, BorshDeserialize, Eq, Ord, PartialEq, PartialOrd, Clone)]
pub(crate) struct BalanceKey {
    pub owner: AccountId,
    pub ctr_id: CtrId,
    pub class_id: ClassId,
}

pub(crate) fn balance_key(owner: AccountId, ctr_id: CtrId, class_id: ClassId) -> BalanceKey {
    BalanceKey {
        owner,
        ctr_id,
        class_id,
    }
}

// macro_rules! borsh_be_integer {
//     ($type: ident) => {
//         impl BorshSerialize for $type {
//             #[inline]
//             fn serialize<W: Write>(&self, writer: &mut W) -> Result<()> {
//                 let bytes = self.to_be_bytes();
//                 writer.write_all(&bytes)
//             }
//         }
//     };
// }

// TODO: implement for
// borsh_be_integer!(CtrId);

// -----------
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
