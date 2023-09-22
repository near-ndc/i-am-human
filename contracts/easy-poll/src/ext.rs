pub use crate::storage::*;
use near_sdk::{ext_contract, AccountId};
use sbt::TokenId;

#[ext_contract(ext_registry)]
trait ExtRegistry {
    // queries
    fn is_human(&self, account: AccountId) -> Vec<(AccountId, Vec<TokenId>)>;
}
