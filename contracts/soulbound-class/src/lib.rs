use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::{
    env, ext_contract, near_bindgen, AccountId, Balance, Gas, PanicOnDefault, PromiseOrValue,
    PromiseResult,
};

/// Balance of one mili NEAR, which is 10^23 Yocto NEAR.
pub const MILI_NEAR: Balance = 1_000_000_000_000_000_000_000;
pub const BLACKLIST_COST: Balance = 5 * MILI_NEAR;
pub const GAS_SBT_QUERY: Gas = Gas(4 * Gas::ONE_TERA.0);

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// Required SBT smart contract
    pub required_sbt: AccountId,
    /// minium amount of tokens a user has to hold to qualify
    pub min_amount: u32,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(required_sbt: AccountId, min_amount: u32) -> Self {
        Self {
            required_sbt,
            min_amount,
        }
    }

    /// returns if given account meets the class criteria
    #[allow(unused_variables)]
    pub fn is_qualified(
        &self,
        account: AccountId,
        payload: Option<String>,
    ) -> PromiseOrValue<bool> {
        ext_sbt::ext(self.required_sbt.clone())
            .sbt_supply_by_owner(account)
            .then(Self::ext(env::current_account_id()).is_qualified_callback())
            .into()
    }

    #[private]
    pub fn is_qualified_callback(&self) -> bool {
        if let PromiseResult::Successful(value) = env::promise_result(0) {
            if let Ok(num) = near_sdk::serde_json::from_slice::<U128>(&value) {
                return num.0 >= self.min_amount as u128;
            }
        }
        false
    }
}

#[ext_contract(ext_sbt)]
pub trait Sbt {
    fn sbt_supply_by_owner(&self, account: AccountId) -> U128;
}
