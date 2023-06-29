use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::{env, near_bindgen, require, AccountId, Balance, PanicOnDefault};

use sbt::*;

pub const MILI_NEAR: Balance = 1_000_000_000_000_000_000__000;
pub const REG_HUMAN_DEPOSIT: Balance = 3 * MILI_NEAR;

pub type HumanSBTs = Vec<(AccountId, Vec<TokenId>)>;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// Accounts authorized to issue new SBT
    pub used_tokens: LookupMap<AccountId, HumanSBTs>,
    /// SBT registry.
    pub registry: AccountId,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    /// @admins: initial set of admins
    /// @ttl: time to live for SBT expire. Must be number in miliseconds.
    #[init]
    pub fn new(registry: AccountId) -> Self {
        Self {
            used_tokens: LookupMap::new(b"1"),
            registry,
        }
    }

    // TODO: once we find a way how to merge human tokens into the args (payload) for
    // `registry.is_human_call`, then we should add here `tokens: Vec<(AccountId, Vec<TokenId>)>`
    #[payable]
    pub fn register_human_token(&mut self, user: AccountId, tokens: HumanSBTs) -> bool {
        require!(
            env::predecessor_account_id() == self.registry,
            "must be called by registry"
        );
        require!(!tokens.is_empty(), "tokens must be a non empty list");
        for ti in &tokens {
            require!(!ti.1.is_empty(), "tokens must be a non empty list");
        }
        if self.used_tokens.contains_key(&user) {
            return false;
        }
        self.used_tokens.insert(&user, &tokens);
        true
    }

    pub fn contains_user(&self, user: AccountId) -> bool {
        self.used_tokens.contains_key(&user)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::{test_utils::VMContextBuilder, testing_env, VMContext};

    fn alice() -> AccountId {
        AccountId::new_unchecked("alice.near".to_string())
    }

    fn issuer1() -> AccountId {
        AccountId::new_unchecked("sbt.n".to_string())
    }

    fn registry() -> AccountId {
        AccountId::new_unchecked("registry".to_string())
    }

    fn setup(predecessor: AccountId, deposit: Balance) -> (VMContext, Contract) {
        let mut ctx = VMContextBuilder::new()
            .predecessor_account_id(predecessor)
            .is_view(false)
            .build();
        if deposit > 0 {
            ctx.attached_deposit = deposit
        }
        testing_env!(ctx.clone());
        let ctr = Contract::new(registry());
        return (ctx, ctr);
    }

    #[test]
    fn register_human_token() {
        let (_, mut ctr) = setup(registry(), REG_HUMAN_DEPOSIT);

        let tokens = vec![(issuer1(), vec![1, 4])];
        assert!(ctr.register_human_token(alice(), tokens.clone()));
        assert_eq!(ctr.used_tokens.get(&alice()).unwrap(), tokens);

        assert!(
            !ctr.register_human_token(alice(), vec![(issuer1(), vec![2])]),
            "second call for the same user should return false"
        );
        assert_eq!(
            ctr.used_tokens.get(&alice()).unwrap(),
            tokens,
            "should not overwrite previous call"
        );
    }

    #[test]
    #[should_panic(expected = "must be called by registry")]
    fn register_human_token_non_registry() {
        let (_, mut ctr) = setup(issuer1(), REG_HUMAN_DEPOSIT);

        let tokens = vec![(issuer1(), vec![1, 4])];
        ctr.register_human_token(alice(), tokens);
    }
}
