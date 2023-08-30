use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::serde_json::value::RawValue;
use near_sdk::{AccountId, BorshStorageKey};
use sbt::{ClassId, SBTs, TokenId};

/// Issuer contract ID based on the SBT Contract address -> u16 map.
pub type IssuerId = u32;

/// Helper structure for keys of the persistent collections.
#[derive(BorshSerialize, BorshStorageKey)]
pub enum StorageKey {
    SbtIssuers,
    SbtIssuersRev,
    Banlist,
    SupplyByOwner,
    SupplyByClass,
    SupplyByIssuer,
    Balances,
    IssuerTokens,
    NextTokenId,
    OngoingSoultTx,
    Flagged,
    AdminsFlagged,
}

#[derive(BorshSerialize, BorshDeserialize, BorshStorageKey, Serialize, Deserialize, PartialEq)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub enum AccountFlag {
    /// Account is "blacklisted" when it was marked as a scam or breaking the IAH rules.
    Blacklisted,
    Verified,
}

/// Composition of issuer address and token id used for indexing
#[derive(BorshSerialize, BorshDeserialize)]
pub(crate) struct IssuerTokenId {
    pub issuer_id: IssuerId,
    pub token: TokenId,
}

#[derive(BorshSerialize, BorshDeserialize, Eq, Ord, PartialEq, PartialOrd, Clone)]
pub(crate) struct BalanceKey {
    pub owner: AccountId,
    pub issuer_id: IssuerId,
    pub class_id: ClassId,
}

#[inline]
pub(crate) fn balance_key(owner: AccountId, issuer_id: IssuerId, class_id: ClassId) -> BalanceKey {
    BalanceKey {
        owner,
        issuer_id,
        class_id,
    }
}

/// `is_human_call` wrapper for passing the payload args to the callback.
#[derive(Serialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug,))]
#[serde(crate = "near_sdk::serde")]
pub struct IsHumanCallbackArgs<'a> {
    pub caller: AccountId,
    pub iah_proof: SBTs,
    pub payload: &'a RawValue,
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::serde_json::{self, json};

    #[test]
    fn is_human_callback_args_serialization() {
        let payload = json!({"nums": [200], "person": {"name": "john", "surname": "Sparrow"}});
        let payload_str = payload.to_string();

        let alice = AccountId::new_unchecked("alice.near".to_string());
        let issuer = AccountId::new_unchecked("issuer.near".to_string());

        let args = IsHumanCallbackArgs {
            caller: alice,
            iah_proof: vec![(issuer, vec![1, 2, 5])],
            payload: &RawValue::from_string(payload_str).unwrap(),
        };

        let args_str = serde_json::to_string(&args).unwrap();
        let expected = r#"{"caller":"alice.near","iah_proof":[["issuer.near",[1,2,5]]],"payload":{"nums":[200],"person":{"name":"john","surname":"Sparrow"}}}"#;

        assert_eq!(expected.to_owned(), args_str);
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
