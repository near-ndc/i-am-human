use near_sdk::borsh::{self, BorshSerialize};
use near_sdk::CryptoHash;

/// Helper structure for keys of the persistent collections.
#[derive(BorshSerialize)]
pub enum StorageKey {
    TokenToOwner,
    TokensPerOwner,
    TokenPerOwnerInner { account_id_hash: CryptoHash },
    TokenMetadataById,
    SBTContractMetadata,
}
