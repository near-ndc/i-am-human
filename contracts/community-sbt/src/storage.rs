use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{AccountId, BorshStorageKey};

/// Helper structure for keys of the persistent collections.
#[derive(BorshSerialize, BorshStorageKey)]
pub enum StorageKey {
    Admins, // deprecated, required for migration
    ContractMetadata,
    MintingAuthority,
}

/// Helper structure for keys of the persistent collections.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(PartialEq, Debug))]
#[serde(crate = "near_sdk::serde")]
pub struct ClassMinters {
    /// if true only iah verifed accounts can obrain the SBT
    pub requires_iah: bool,
    /// accounts allowed to mint the SBT
    pub minters: Vec<AccountId>,
    /// time to live in ms. Overwrites metadata.expire_at.
    pub ttl: u64,
}
