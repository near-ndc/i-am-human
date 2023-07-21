use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::{env, near_bindgen, require, AccountId, PanicOnDefault};

pub use crate::storage::*;

mod storage;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// Account authorized to add new minting authority
    pub admin: AccountId,
    /// map of classId -> to set of accounts authorized to mint
    pub polls: UnorderedMap<u64, Poll>,
    /// SBT registry.
    pub registry: AccountId,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    /// @admin: account authorized to add new minting authority
    /// @ttl: time to live for SBT expire. Must be number in miliseconds.
    #[init]
    pub fn new(admin: AccountId, registry: AccountId) -> Self {
        Self {
            admin,
            polls: UnorderedMap::new(StorageKey::Polls),
            registry,
        }
    }

    /**********
     * QUERIES
     **********/

    // user can update the poll if starts_at > now
    // it panics if
    // - user tries to create an invalid poll
    // - if poll aready exists and starts_at < now
    pub fn create_poll(&self) -> PollId {
        unimplemented!();
    }

    // user can change his vote when the poll is still active.
    // it panics if
    // - poll not found
    // - poll not active
    // - poll.verified_humans_only is true, and user is not verified on IAH
    // - user tries to vote with an invalid answer to a question
    pub fn vote(&self) {
        unimplemented!();
    }

    // returns None if poll is not found
    pub fn result(poll_id: usize) -> bool {
        unimplemented!();
    }
    /**********
     * INTERNAL
     **********/

    fn assert_admin(&self) {
        require!(self.admin == env::predecessor_account_id(), "not an admin");
    }
}

#[cfg(test)]
mod tests {
    use near_sdk::{test_utils::VMContextBuilder, testing_env, AccountId, Balance, VMContext};
    use sbt::{ClassId, ContractMetadata, TokenMetadata};

    use crate::Contract;

    fn alice() -> AccountId {
        AccountId::new_unchecked("alice.near".to_string())
    }

    fn registry() -> AccountId {
        AccountId::new_unchecked("registry.near".to_string())
    }

    fn admin() -> AccountId {
        AccountId::new_unchecked("admin.near".to_string())
    }

    fn setup(predecessor: &AccountId) -> (VMContext, Contract) {
        let mut ctx = VMContextBuilder::new()
            .predecessor_account_id(admin())
            .block_timestamp(0)
            .is_view(false)
            .build();
        testing_env!(ctx.clone());
        let mut ctr = Contract::new(admin(), registry());
        ctx.predecessor_account_id = predecessor.clone();
        testing_env!(ctx.clone());
        return (ctx, ctr);
    }

    #[test]
    fn assert_admin() {
        let (mut ctx, mut ctr) = setup(&admin());
        ctr.assert_admin();
    }
}
