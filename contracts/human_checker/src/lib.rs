use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    env, near_bindgen, require, AccountId, Balance, Gas, NearSchema, PanicOnDefault, Promise,
    PromiseOrValue, ONE_NEAR,
};

use sbt::*;

pub const MILI_NEAR: Balance = 1_000_000_000_000_000_000_000;
pub const REG_HUMAN_DEPOSIT: Balance = 3 * MILI_NEAR;
pub const FAILURE_CALLBACK_GAS: Gas = Gas(3 * Gas::ONE_TERA.0);
/// maximum time for proposal voting in milliseconds.
pub const VOTING_DURATION: u64 = 20_000;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// Accounts authorized to issue new SBT
    pub used_tokens: LookupMap<AccountId, SBTs>,
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

    #[payable]
    pub fn register_human_token(
        &mut self,
        caller: AccountId,
        iah_proof: SBTs,
        payload: RegisterHumanPayload,
    ) -> PromiseOrValue<bool> {
        env::log_str(&format!(
            "register token for {}, memo={}",
            caller, payload.memo
        ));

        let deposit = env::attached_deposit();
        if deposit < 2 * ONE_NEAR {
            return PromiseOrValue::Promise(
                Promise::new(caller)
                    .transfer(deposit)
                    .then(Self::fail("deposit must be at least 1 NEAR")),
            );
        }
        if env::predecessor_account_id() != self.registry {
            return PromiseOrValue::Promise(
                Promise::new(caller)
                    .transfer(deposit)
                    .then(Self::fail("must be called by registry")),
            );
        }
        if payload.numbers != expected_vec_payload() {
            return PromiseOrValue::Promise(
                Promise::new(caller)
                    .transfer(deposit)
                    .then(Self::fail("wrong payload")),
            );
        }

        if iah_proof.is_empty() {
            return PromiseOrValue::Promise(
                Promise::new(caller)
                    .transfer(deposit)
                    .then(Self::fail("not a human")),
            );
        }
        for (_, tokens) in &iah_proof {
            if tokens.is_empty() {
                return PromiseOrValue::Promise(
                    Promise::new(caller)
                        .transfer(deposit)
                        .then(Self::fail("bad response, expected non empty token list")),
                );
            }
        }
        if self.used_tokens.contains_key(&caller) {
            return near_sdk::PromiseOrValue::Value(false);
        }
        self.used_tokens.insert(&caller, &iah_proof);
        near_sdk::PromiseOrValue::Value(true)
    }

    pub fn recorded_sbts(&self, user: AccountId) -> Option<SBTs> {
        self.used_tokens.get(&user)
    }

    fn fail(reason: &str) -> Promise {
        Self::ext(env::current_account_id())
            .with_static_gas(FAILURE_CALLBACK_GAS)
            .on_failure(reason.to_string())
    }

    #[private]
    pub fn on_failure(&mut self, error: String) {
        env::panic_str(&error)
    }
    /// Simulates a governance voting. Every valid human (as per IAH registry) can vote.
    /// To avoid double voting by an account who is doing soul_transfer while a proposal is
    /// active, we require that voing must be called through `iah_registry.is_human_call_lock`.
    /// We check that the caller set enough `lock_duration` for soul transfers.
    /// Arguments:
    /// * `caller`: account ID making a vote (passed by `iah_registry.is_human_call`)
    /// * `locked_until`: time in milliseconds, untile when the caller is locked for soul
    ///   transfers (reported by `iah_registry.is_human_call`).
    /// * `iah_proof`: proof of humanity. It's not required and will be ignored.
    /// * `payload`: the proposal ID and the vote (approve or reject).
    #[payable]
    pub fn vote(
        &mut self,
        caller: AccountId,
        locked_until: u64,
        #[allow(unused_variables)] iah_proof: Option<SBTs>,
        payload: VotePayload,
    ) {
        // for this simulation we imagine that every proposal ID is valid and it's finishing
        // at "now" + VOTING_DURATION
        require!(
            env::predecessor_account_id() == self.registry,
            "must be called by registry"
        );
        require!(
            locked_until >= env::block_timestamp_ms() + VOTING_DURATION,
            "account not locked for soul transfer for sufficient amount of time"
        );
        require!(payload.prop_id > 0, "invalid proposal id");
        require!(
            payload.vote == "approve" || payload.vote == "reject",
            "invalid vote: must be either 'approve' or 'reject'"
        );

        env::log_str(&format!(
            "VOTED: voter={}, proposal={}, vote={}",
            caller, payload.prop_id, payload.vote,
        ));
    }
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, Clone))]
#[serde(crate = "near_sdk::serde")]
pub struct RegisterHumanPayload {
    pub memo: String,
    pub numbers: Vec<u32>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, Clone))]
#[serde(crate = "near_sdk::serde")]
pub struct VotePayload {
    pub prop_id: u32,
    pub vote: String,
}

pub(crate) fn expected_vec_payload() -> Vec<u32> {
    vec![2, 3, 5, 7, 11]
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
        (ctx, ctr)
    }

    // #[test]
    // fn register_human_token() {
    //     let (_, mut ctr) = setup(registry(), REG_HUMAN_DEPOSIT);

    //     let tokens = vec![(issuer1(), vec![1, 4])];
    //     let payload = RegisterHumanPayload {
    //         memo: "checking alice".to_owned(),
    //         numbers: expected_vec_payload(),
    //     };
    //     assert!(ctr.register_human_token(alice(), tokens.clone(), payload.clone()).);
    //     assert_eq!(ctr.used_tokens.get(&alice()).unwrap(), tokens);

    //     assert!(
    //         //  !ctr.register_human_token(alice(), vec![(issuer1(), vec![2])], payload),
    //         "second call for the same user should return false"
    //     );
    //     assert_eq!(
    //         ctr.used_tokens.get(&alice()).unwrap(),
    //         tokens,
    //         "should not overwrite previous call"
    //     );
    // }

    // #[test]
    // #[should_panic(expected = "must be called by registry")]
    // fn register_human_token_non_registry() {
    //     let (_, mut ctr) = setup(issuer1(), REG_HUMAN_DEPOSIT);

    //     let tokens = vec![(issuer1(), vec![1, 4])];
    //     ctr.register_human_token(
    //         alice(),
    //         tokens,
    //         RegisterHumanPayload {
    //             memo: "registering alice".to_owned(),
    //             numbers: expected_vec_payload(),
    //         },
    //     );
    // }
}
