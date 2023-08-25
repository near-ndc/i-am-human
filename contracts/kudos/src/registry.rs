use near_sdk::json_types::Base64VecU8;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{ext_contract, AccountId, Gas, Promise};

pub type TokenId = u64;

pub const IS_HUMAN_GAS: Gas = Gas(12 * Gas::ONE_TERA.0);

/// TokenMetadata defines attributes for each SBT token.
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct TokenMetadata {
    pub class: u64,                          // token class
    pub issued_at: Option<u64>, // When token was issued or minted, Unix epoch in milliseconds
    pub expires_at: Option<u64>, // When token expires, Unix epoch in milliseconds
    pub reference: Option<String>, // URL to an off-chain JSON file with more info.
    pub reference_hash: Option<Base64VecU8>, // Base64-encoded sha256 hash of JSON from reference field. Required if `reference` is included.
}

#[derive(Deserialize, Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct OwnedToken {
    pub token: u64,
    pub metadata: TokenMetadata,
}

#[ext_contract(ext_sbtreg)]
pub trait ExtSbtRegistry {
    /*
    fn is_human_call(
        &mut self,
        account: AccountId,
        ctr: AccountId,
        function: String,
        args: Base64VecU8,
    ) -> PromiseOrValue<bool>;
    */

    fn is_human(&self, account: AccountId) -> Vec<(AccountId, Vec<TokenId>)>;

    fn sbt_mint(&mut self, token_spec: Vec<(AccountId, Vec<TokenMetadata>)>) -> Promise;
}
