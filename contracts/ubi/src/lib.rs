use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::json_types::U128;
use near_sdk::{
    env, ext_contract, near_bindgen, require, AccountId, Balance, Gas, PanicOnDefault, Promise,
    PromiseOrValue, PromiseResult,
};

/// Balance of one mili NEAR, which is 10^23 Yocto NEAR.
pub const MILI_NEAR: Balance = 1_000_000_000_000_000_000_000;
pub const BLACKLIST_COST: Balance = 5 * MILI_NEAR;
pub const GAS_SBT_QUERY: Gas = Gas(4 * Gas::ONE_TERA.0);

/// DAY in seconds
pub const DAY: u64 = 3600 * 24;
/// TTL for proof of humanity check = 100 days
pub const TTL: u64 = DAY * 100;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// class verifying humanity
    pub class: AccountId,
    /// daily emission
    pub emission: u128,

    /// map of registered humans, and their record
    pub humans: UnorderedMap<AccountId, UbiCheck>,
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct UbiCheck {
    /// last unix timestamp (in seconds) when last claim was made
    pub last_claim: u64,
    /// expire time of humanity verificaion as a unix timestamp (in seconds)
    pub expires_at: u64,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(human_class: AccountId, emission: U128) -> Self {
        Self {
            class: human_class,
            emission: emission.0,
            humans: UnorderedMap::new(b'h'),
        }
    }

    /// registers a new user
    pub fn register(&mut self) -> PromiseOrValue<bool> {
        let user = env::predecessor_account_id();
        ext_class::ext(self.class.clone())
            .is_qualified(user.clone(), None)
            .then(Self::ext(env::current_account_id()).register_callback(user))
            .into()
    }

    pub fn claim(&mut self) {
        let user = env::predecessor_account_id();
        let mut check = self.humans.get(&user).expect("user not registered");
        let now = env::block_timestamp_ms() / 1_000;
        require!(check.expires_at > now, "human proof expired");
        require!(
            check.last_claim + DAY < now,
            "UBI already claimed in past 24h"
        );

        check.last_claim = now;
        self.humans.insert(&user, &check);
        // transfer NEAR UBI
        Promise::new(user).transfer(self.emission);
    }

    #[private]
    pub fn register_callback(&mut self, account: AccountId) -> bool {
        if let PromiseResult::Successful(value) = env::promise_result(0) {
            if let Ok(ok) = near_sdk::serde_json::from_slice::<bool>(&value) {
                if !ok {
                    return false;
                }
                let now = env::block_timestamp_ms() / 1_000;
                self.humans.insert(
                    &account,
                    &UbiCheck {
                        last_claim: now - DAY - 1,
                        expires_at: now + TTL,
                    },
                );
                return true;
            }
        }
        false
    }
}

#[ext_contract(ext_class)]
pub trait HumanClass {
    fn is_qualified(&self, account: AccountId, payload: Option<String>) -> PromiseOrValue<bool>;
}
