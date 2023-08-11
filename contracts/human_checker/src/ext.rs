use near_sdk::json_types::Base64VecU8;
use near_sdk::serde::Deserialize;
use near_sdk::{ext_contract, AccountId, PromiseOrValue};

// imports needed for conditional derive (required for tests)
#[allow(unused_imports)]
use near_sdk::serde::Serialize;

#[ext_contract(ext_sbtreg)]
pub trait ExtSbtRegistry {
    fn is_human_call(
        &mut self,
        account: AccountId,
        ctr: AccountId,
        function: String,
        payload: String,
    ) -> PromiseOrValue<bool>;
}
