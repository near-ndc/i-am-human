use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::Base64VecU8;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{require, AccountId};

#[allow(unused_imports)]
use near_sdk::NearSchema;

use crate::*;

/// ContractMetadata defines contract wide attributes, which describes the whole contract.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(
    not(target_arch = "wasm32"),
    derive(Debug, PartialEq, Clone, NearSchema)
)]
pub struct ContractMetadata {
    /// Version with namespace, example: "sbt-1.0.0". Required.
    pub spec: String,
    /// Issuer Name, required, ex. "Mosaics"
    pub name: String,
    /// Issuer symbol which can be used as a token symbol, eg Ⓝ, ₿, BTC, MOSAIC ...
    pub symbol: String,
    /// Icon content (SVG) or a link to an Icon. If it doesn't start with a scheme (eg: https://)
    /// then `base_uri` should be prepended.
    pub icon: Option<String>,
    /// URI prefix which will be prepended to other links which don't start with a scheme
    /// (eg: ipfs:// or https:// ...).
    pub base_uri: Option<String>,
    /// JSON or an URL to a JSON file with more info. If it doesn't start with a scheme
    /// (eg: https://) then base_uri should be prepended.
    pub reference: Option<String>,
    /// Base64-encoded sha256 hash of JSON from reference field. Required if `reference` is included.
    pub reference_hash: Option<Base64VecU8>,
}

/// ClassMetadata describes an issuer class.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(
    not(target_arch = "wasm32"),
    derive(Debug, PartialEq, Clone, NearSchema)
)]
pub struct ClassMetadata {
    /// Issuer class name. Required.
    pub name: String,
    /// If defined, should be used instead of `contract_metadata.symbol`.
    pub symbol: Option<String>,
    /// Icon content (SVG) or a link to an Icon. If it doesn't start with a scheme (eg: https://)
    /// then `contract_metadata.base_uri` should be prepended.
    pub icon: Option<String>,
    /// JSON or an URL to a JSON file with more info. If it doesn't start with a scheme
    /// (eg: https://) then base_uri should be prepended.
    pub reference: Option<String>,
    /// Base64-encoded sha256 hash of JSON from reference field. Required if `reference` is included.
    pub reference_hash: Option<Base64VecU8>,
}

/// Versioned token metadata
#[derive(BorshDeserialize, BorshSerialize, Serialize)]
#[cfg_attr(test, derive(Debug, Clone))]
#[serde(crate = "near_sdk::serde")]
pub enum VerTokenMetadata {
    V1(TokenMetadata),
}

/// TokenMetadata defines attributes for each SBT token.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(
    not(target_arch = "wasm32"),
    derive(Debug, PartialEq, Clone, NearSchema)
)]
pub struct TokenMetadata {
    /// token class, must be non zero.
    pub class: ClassId,
    /// When the token was issued or minted, Unix time in milliseconds
    pub issued_at: Option<u64>,
    /// When the token expires, Unix time in milliseconds
    pub expires_at: Option<u64>,
    /// JSON or an URL to a JSON file with more info. If it doesn't start with a scheme
    /// (eg: https://) then base_uri should be prepended.
    pub reference: Option<String>,
    /// Base64-encoded sha256 hash of JSON from reference field. Required if `reference` is included.
    pub reference_hash: Option<Base64VecU8>,
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

    pub fn expires_at(&self) -> Option<u64> {
        match self {
            VerTokenMetadata::V1(x) => x.expires_at,
        }
    }
}

impl From<TokenMetadata> for VerTokenMetadata {
    fn from(m: TokenMetadata) -> Self {
        VerTokenMetadata::V1(m)
    }
}

/// Full information about the token
#[derive(BorshDeserialize, BorshSerialize, Serialize)]
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
#[cfg_attr(
    not(target_arch = "wasm32"),
    derive(Debug, PartialEq, Clone, NearSchema)
)]
pub struct OwnedToken {
    pub token: TokenId,
    pub metadata: TokenMetadata,
}

/// Full information about the token
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(
    not(target_arch = "wasm32"),
    derive(Debug, PartialEq, Clone, NearSchema)
)]
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
