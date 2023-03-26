mod events;
mod metadata;

use near_sdk::{AccountId, Balance, Gas};

pub use crate::events::*;
pub use crate::metadata::*;

// u64 capacity is more than 1e19. If we will mint 10'000 SBTs per second, than it will take us
// 58'494'241 years to get into the capacity.
// Today, the JS integer limit is `2^53-1 ~ 9e15`. It will take us 28'561 years to fill that when minting
// 10'000 SBTs per second.
// So, we don't need to u128 nor a String type.
pub type TokenId = u64;

pub type KindId = u64;

/// This spec can be treated like a version of the standard.
pub const SPEC_VERSION: &str = "1.0.0";
/// This is the name of the SBT standard we're using
pub const STANDARD_NAME: &str = "nep393";

/// Balance of one mili NEAR, which is 10^23 Yocto NEAR.
pub const MILI_NEAR: Balance = 1_000_000_000_000_000_000_000;

pub const BLACKLIST_COST: Balance = 5 * MILI_NEAR;
pub const GAS_FOR_BLACKLIST: Gas = Gas(6 * Gas::ONE_TERA.0);

trait SBTRegistry {
    /**********
     * QUERIES
     **********/

    /// get the information about specific token ID
    fn sbt(&self, ctr: AccountId, token_id: TokenId) -> Option<Token>;

    /// returns total amount of tokens minted by this contract
    fn sbt_total_supply(&self, ctr: AccountId) -> u64;

    /// returns total amount of tokens of given kind minted by this contract
    fn sbt_total_supply_by_kind(&self, ctr: AccountId, kind: KindId) -> u64;

    /// returns total supply of SBTs for a given owner
    fn sbt_supply_by_owner(&self, ctr: AccountId, account: AccountId) -> u64;

    /// returns true if the `account` has a token of a given `kind`.
    fn sbt_supply_by_kind(&self, ctr: AccountId, account: AccountId, kind: KindId) -> bool;

    /// Query sbt tokens. If `from_index` is not specified, then `from_index` should be assumed
    /// to be the first valid token id.
    fn sbt_tokens(
        &self,
        ctr: AccountId,
        from_index: Option<u64>,
        limit: Option<u32>,
    ) -> Vec<TokenId>;

    /// Query sbt tokens by owner
    /// If `from_kind` is not specified, then `from_kind` should be assumed to be the first
    /// valid kind id.
    fn sbt_tokens_by_owner(
        &self,
        ctr: AccountId,
        account: AccountId,
        from_kind: Option<u64>,
        limit: Option<u32>,
    ) -> Vec<TokenId>;

    /*************
     * Transactions
     *************/

    /// Creates a new, unique token and assigns it to the `receiver`.
    /// Must be called by an SBT contract.
    /// Must emit NEP-171 compatible `Mint` event.
    /// Must provide enough NEAR to cover registry storage cost.
    /// The arguments to this function can vary, depending on the use-case.
    /// `kind` is provided as an explicit argument and it must overwrite `metadata.kind`.
    /// Requires attaching enough tokens to cover the storage growth.
    // #[payable]
    fn sbt_mint(
        &mut self,
        account: AccountId,
        kind: Option<u64>,
        metadata: TokenMetadata,
    ) -> TokenId;

    /// sbt_recover reassigns all tokens from the old owner to a new owner,
    /// and registers `old_owner` to a burned addresses registry.
    /// Must be called by an SBT contract.
    /// Must emit `Recover` event.
    /// Must be called by an operator.
    /// Must provide enough NEAR to cover registry storage cost.
    /// Requires attaching enough tokens to cover the storage growth.
    // #[payable]
    fn sbt_recover(&mut self, from: AccountId, to: AccountId);

    /// sbt_renew will update the expire time of provided tokens.
    /// `expires_at` is a unix timestamp (in seconds).
    /// Must be called by an SBT contract.
    /// Must emit `Renew` event.
    fn sbt_renew(&mut self, tokens: Vec<TokenId>, expires_at: u64, memo: Option<String>);

    /// Revokes SBT, could potentailly burn it or update the expire time.
    /// Must be called by an SBT contract.
    /// Must emit `Revoke` event.
    /// Returns true if a token_id is a valid, active SBT. Otherwise returns false.
    fn sbt_revoke(&mut self, token_id: u64) -> bool;

    /// Transfers atomically all SBT tokens from one account to another account.
    /// Must be an SBT holder.
    // #[payable]
    fn sbt_soul_transfer(&mut self, to: AccountId) -> bool;
}
