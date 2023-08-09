pub use crate::storage::*;
use near_sdk::{ext_contract, AccountId};
use sbt::TokenId;

use crate::PollError;

#[ext_contract(ext_registry)]
trait ExtRegistry {
    // queries

    fn is_human(&self, account: AccountId) -> Vec<(AccountId, Vec<TokenId>)>;
}
// #[ext_contract(ext_self)]
// pub trait ExtSelf {
//     fn on_human_verifed(
//         &mut self,
//         poll_id: PollId,
//         answers: Vec<Option<Answer>>,
//     ) -> Result<(), PollError>;
// }
