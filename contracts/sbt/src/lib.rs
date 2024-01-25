mod events;
mod metadata;

use near_sdk::json_types::Base64VecU8;
use near_sdk::{ext_contract, AccountId};

pub use crate::events::*;
pub use crate::metadata::*;

/// This spec can be treated like a version of the standard.
pub const SPEC_VERSION: &str = "1.0.0";
/// This is the name of the SBT standard we're using
pub const STANDARD_NAME: &str = "nep393";

/// 1s in nano seconds.
pub const SECOND: u64 = 1_000_000_000;

// u64 capacity is more than 1e19. If we will mint 10'000 SBTs per second, than it will take us
// 58'494'241 years to get into the capacity.
// Today, the JS integer limit is `2^53-1 ~ 9e15`. It will take us 28'561 years to fill that when minting
// 10'000 SBTs per second.
// So, we don't need to u128 nor a String type.
/// Identifier of a token. There must be no 2 same tokens issued by the same contract, even
/// if other token with same TokenId was burned, or the differe by the `ClassId`.
/// Minimum valid `TokenId` must be 1.
pub type TokenId = u64;

/// The `ClassId` defines a class (category) of SBT set issued from the same contract.
/// SBT tokens can't be fractionized. Also, by definition there should be only one of a token
/// per token class per user. We propose that the SBT Standard will support the multi-token
/// idea from the get go. In a traditional NFT scenario, one contract will only issue tokens
/// of the single class.
/// Minimum valid `ClassId` must be 1.
pub type ClassId = u64;

/// Collection of SBTs serialized as list of pairs: (Issuer Account, Vector of Token IDs).
/// This is used for code size and processing efficiency.
pub type SBTs = Vec<(AccountId, Vec<TokenId>)>;

/// List of pairs: (Issuer Account, Vector of Class IDs).
/// This is used to create class sets used to specify required token classes,
/// like set of tokens required to be verified as IAH
pub type ClassSet = Vec<(AccountId, Vec<ClassId>)>;

/// SBTIssuer is the minimum required interface to be implemented by issuer.
/// Other methods, such as a mint function, which requests the registry to proceed with token
/// minting, is specific to an Issuer implementation (similarly, mint is not part of the FT
/// standard).
pub trait SBTIssuer {
    /// returns contract metadata
    fn sbt_metadata(&self) -> ContractMetadata;
    /// returns sbt class metadata, or None if class is not found.
    fn sbt_class_metadata(&self, class: ClassId) -> Option<ClassMetadata>;
}

pub trait SBTRegistry {
    /**********
     * QUERIES
     **********/

    /// Get the information about specific token ID issued by the `issuer` SBT contract.
    fn sbt(&self, issuer: AccountId, token: TokenId) -> Option<Token>;

    /// Get the information about list of token IDs issued by the SBT `issuer`.
    /// If token ID is not found, `None` is set in the specific return index.
    fn sbts(&self, issuer: AccountId, token: Vec<TokenId>) -> Vec<Option<Token>>;

    /// Query class ID for each token ID issued by the SBT `issuer`.
    /// If token ID is not found, `None` is set in the specific return index.
    fn sbt_classes(&self, issuer: AccountId, tokens: Vec<TokenId>) -> Vec<Option<ClassId>>;

    /// Returns total amount of tokens issued by `issuer` SBT contract, including expired
    /// tokens. Depending on the implementation, if a revoke removes a token, then it should
    /// not be included in the supply.
    fn sbt_supply(&self, issuer: AccountId) -> u64;

    /// Returns total amount of tokens of given class minted by `issuer`. See `sbt_supply` for
    /// information about revoked tokens.
    fn sbt_supply_by_class(&self, issuer: AccountId, class: ClassId) -> u64;

    /// Returns total supply of SBTs for a given owner. See `sbt_supply` for information about
    /// revoked tokens.
    /// If class is specified, returns only owner supply of the given class -- must be 0 or 1.
    fn sbt_supply_by_owner(
        &self,
        account: AccountId,
        issuer: AccountId,
        class: Option<ClassId>,
    ) -> u64;

    /// Query sbt tokens issued by a given contract.
    /// `limit` specifies the upper limit of how many tokens we want to return.
    /// If `from_token` is not specified, then `from_token` should be assumed
    /// to be the first valid token id. If `with_expired` if is set to `false` or to `None` then
    /// all tokens are returned.
    fn sbt_tokens(
        &self,
        issuer: AccountId,
        from_token: Option<u64>,
        limit: Option<u32>,
        with_expired: Option<bool>,
    ) -> Vec<Token>;

    /// Query SBT tokens by owner.
    /// `limit` specifies the upper limit of how many tokens we want to return.
    /// If `from_class` is not specified, then `from_class` should be assumed to be the first
    /// valid class id. If `with_expired` if is set to `false` or `None` then all tokens are returned.
    /// Returns list of pairs: `(Contract address, list of token IDs)`.
    /// If `with_expired` is set to `true` then all the tokens are returned including expired ones
    /// otherwise only non-expired tokens are returned.
    fn sbt_tokens_by_owner(
        &self,
        account: AccountId,
        issuer: Option<AccountId>,
        from_class: Option<u64>,
        limit: Option<u32>,
        with_expired: Option<bool>,
    ) -> Vec<(AccountId, Vec<OwnedToken>)>;

    /// checks if an `account` was banned by the registry.
    fn is_banned(&self, account: AccountId) -> bool;

    /*************
     * Transactions
     *************/

    /// Creates a new, unique token and assigns it to the `receiver`.
    /// `token_spec` is a vector of pairs: owner AccountId and TokenMetadata.
    /// Each TokenMetadata must have non zero `class`.
    /// Must be called by an SBT contract.
    /// Must emit `Mint` event.
    /// Must provide enough NEAR to cover registry storage cost.
    // #[payable]
    fn sbt_mint(&mut self, token_spec: Vec<(AccountId, Vec<TokenMetadata>)>) -> Vec<TokenId>;

    /// sbt_recover reassigns all tokens issued by the caller, from the old owner to a new owner.
    /// Must be called by a valid SBT issuer.
    /// Must emit `Recover` event once all the tokens have been recovered.
    /// Requires attaching enough tokens to cover the storage growth.
    /// Returns the amount of tokens recovered and a boolean: `true` if the whole
    /// process has finished, `false` when the process has not finished and should be
    /// continued by a subsequent call. User must keep calling the `sbt_recover` until `true`
    /// is returned.
    // #[payable]
    fn sbt_recover(&mut self, from: AccountId, to: AccountId) -> (u32, bool);

    /// sbt_renew will update the expire time of provided tokens.
    /// `expires_at` is a unix timestamp in miliseconds.
    /// Must be called by an SBT contract.
    /// Must emit `Renew` event.
    fn sbt_renew(&mut self, tokens: Vec<TokenId>, expires_at: u64);

    /// Revokes SBT by burning the token or updating its expire time.
    /// Must be called by an SBT contract.
    /// Must emit `Revoke` event.
    /// Must also emit `Burn` event if the SBT tokens are burned (removed).
    fn sbt_revoke(&mut self, tokens: Vec<TokenId>, burn: bool);

    /// Revokes all owners SBTs issued by the caller either by burning or updating their expire time.
    /// Must be called by an SBT contract.
    /// Must emit `Revoke` event.
    /// Must also emit `Burn` event if the SBT tokens are burned (removed).
    fn sbt_revoke_by_owner(&mut self, owner: AccountId, burn: bool) -> bool;

    /// Allows issuer to update token metadata reference and reference_hash.
    /// * `updates` is a list of triples: (token ID, reference, reference hash).
    /// Must emit `token_reference` event.
    /// Panics if any of the token IDs don't exist.
    fn sbt_update_token_references(
        &mut self,
        updates: Vec<(TokenId, Option<String>, Option<Base64VecU8>)>,
    );
}

// ext_registry is a helper to make cross contract registry calls
#[ext_contract(ext_registry)]
trait ExtRegistry {
    fn sbt_mint(&mut self, token_spec: Vec<(AccountId, Vec<TokenMetadata>)>) -> Vec<TokenId>;
    fn sbt_mint_iah(&mut self, token_spec: Vec<(AccountId, Vec<TokenMetadata>)>) -> Vec<TokenId>;
    fn sbt_renew(&mut self, tokens: Vec<TokenId>, expires_at: u64);
    fn sbt_revoke(&mut self, tokens: Vec<TokenId>, burn: bool);

    // queries

    fn is_human(&self, account: AccountId) -> Vec<(AccountId, Vec<TokenId>)>;
    fn sbt(&self, issuer: AccountId, token: TokenId) -> Option<Token>;
    fn sbts(&self, issuer: AccountId, tokens: Vec<TokenId>) -> Vec<Option<Token>>;
    fn sbt_classes(&self, issuer: AccountId, tokens: Vec<TokenId>) -> Vec<Option<ClassId>>;
}
