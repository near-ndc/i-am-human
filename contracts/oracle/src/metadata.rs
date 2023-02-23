use near_sdk::json_types::Base64VecU8;

use crate::*;
pub type TokenId = u64;

/// This spec can be treated like a version of the standard.
pub const METADATA_SPEC: &str = "1.0.0";
/// This is the name of the SBT standard we're using
pub const SBT_STANDARD_NAME: &str = "nepTODO";

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct SBTContractMetadata {
    pub spec: String,              // required, essentially a version like "sbt-1.0.0"
    pub name: String,              // required, ex. "Mosaics"
    pub symbol: String,            // required, ex. "MOSAIC"
    pub icon: Option<String>,      // Data URL
    pub base_uri: Option<String>, // Centralized gateway known to have reliable access to decentralized storage assets referenced by `reference` or `media` URLs
    pub reference: Option<String>, // URL to a JSON file with more info
    pub reference_hash: Option<Base64VecU8>, // Base64-encoded sha256 hash of JSON from reference field. Required if `reference` is included.
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct TokenMetadata {
    pub issued_at: Option<u64>, // When token was issued or minted, Unix epoch in milliseconds
    pub expires_at: Option<u64>, // When token expires, Unix epoch in milliseconds
    pub reference: Option<String>, // URL to an off-chain JSON file with more info.
    pub reference_hash: Option<Base64VecU8>, // Base64-encoded sha256 hash of JSON from reference field. Required if `reference` is included.
}

/// Full information about the token
#[derive(BorshDeserialize, BorshSerialize)]
pub struct TokenData {
    pub owner: AccountId,
    pub metadata: TokenMetadata,
}

/// Full information about the token
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Token {
    pub token_id: TokenId,
    pub owner_id: AccountId,
    pub metadata: TokenMetadata,
}

pub trait SBTMetadata {
    //view call for returning the contract metadata
    fn sbt_metadata(&self) -> SBTContractMetadata;
}

#[near_bindgen]
impl SBTMetadata for Contract {
    fn sbt_metadata(&self) -> SBTContractMetadata {
        self.metadata.get().unwrap()
    }
}
