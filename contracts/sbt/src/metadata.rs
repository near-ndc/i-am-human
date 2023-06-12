use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::Base64VecU8;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{require, AccountId};

use crate::*;

/// ContractMetadata defines contract wide attributes, which describes the whole contract.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Clone))]
pub struct ContractMetadata {
    pub spec: String,              // required, essentially a version like "sbt-1.0.0"
    pub name: String,              // required, ex. "Mosaics"
    pub symbol: String,            // required, ex. "MOSAIC"
    pub icon: Option<String>,      // Data URL
    pub base_uri: Option<String>, // Centralized gateway known to have reliable access to decentralized storage assets referenced by `reference` or `media` URLs
    pub reference: Option<String>, // URL to a JSON file with more info
    pub reference_hash: Option<Base64VecU8>, // Base64-encoded sha256 hash of JSON from reference field. Required if `reference` is included.
}

/// Versioned token metadata
#[derive(BorshDeserialize, BorshSerialize, Serialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub enum VerTokenMetadata {
    V1(TokenMetadata),
}

/// TokenMetadata defines attributes for each SBT token.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq))]
pub struct TokenMetadata {
    pub class: ClassId,                      // token class
    pub issued_at: Option<u64>, // When token was issued or minted, Unix epoch in milliseconds
    pub expires_at: Option<u64>, // When token expires, Unix epoch in milliseconds
    pub reference: Option<String>, // URL to an off-chain JSON file with more info.
    pub reference_hash: Option<Base64VecU8>, // Base64-encoded sha256 hash of JSON from reference field. Required if `reference` is included.
}

impl VerTokenMetadata {
    pub fn v1(self) -> TokenMetadata {
        match self {
            VerTokenMetadata::V1(x) => x,
        }
    }

    pub fn class_id(&self) -> ClassId {
        match self {
            VerTokenMetadata::V1(x) => x.class,
        }
    }
}

impl From<TokenMetadata> for VerTokenMetadata {
    fn from(m: TokenMetadata) -> Self {
        VerTokenMetadata::V1(m)
    }
}

/// Full information about the token
#[derive(BorshDeserialize, BorshSerialize, Serialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct TokenData {
    pub owner: AccountId,
    pub metadata: VerTokenMetadata,
}

impl TokenData {
    pub fn to_token(self, token: TokenId) -> Token {
        let metadata: TokenMetadata = self.metadata.v1();
        Token {
            token,
            metadata,
            owner: self.owner,
        }
    }
}

/// token data for sbt_tokens_by_owner response
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Clone))]
pub struct OwnedToken {
    pub token: TokenId,
    pub metadata: TokenMetadata,
}

/// Full information about the token
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Clone))]
pub struct Token {
    pub token: TokenId,
    pub owner: AccountId,
    pub metadata: TokenMetadata,
}

impl ContractMetadata {
    pub fn assert_valid(&self) {
        require!(self.spec == crate::SPEC_VERSION, "Spec is not NFT metadata");
        require!(
            !self.name.is_empty() && !self.symbol.is_empty(),
            "name and spec must be a non empty string"
        );
        require!(
            self.reference.is_some() == self.reference_hash.is_some(),
            "Reference and reference hash must be present"
        );
        if let Some(reference_hash) = &self.reference_hash {
            require!(reference_hash.0.len() == 32, "Hash has to be 32 bytes");
        }
    }
}

impl TokenMetadata {
    pub fn assert_valid(&self) {
        // require!(self.media.is_some() == self.media_hash.is_some());
        // if let Some(media_hash) = &self.media_hash {
        //     require!(media_hash.0.len() == 32, "Media hash has to be 32 bytes");
        // }

        require!(self.reference.is_some() == self.reference_hash.is_some());
        if let Some(reference_hash) = &self.reference_hash {
            require!(
                reference_hash.0.len() == 32,
                "Reference hash has to be 32 bytes"
            );
        }
    }
}
