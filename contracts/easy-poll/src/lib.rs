use std::fmt::format;
use std::future::Future;

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
    pub polls: UnorderedMap<PollId, Poll>,
    pub results: UnorderedMap<PollId, Vec<PollQuestionAnswer>>,
    pub poll_questions_results: UnorderedMap<PollId, Vec<PollQuestionAnswer>>,
    /// SBT registry.
    pub registry: AccountId,
    /// next poll id
    pub next_poll_id: PollId,
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
            results: UnorderedMap::new(StorageKey::Results),
            poll_questions_results: UnorderedMap::new(StorageKey::Answers),
            registry,
            next_poll_id: 0,
        }
    }

    /**********
     * QUERIES
     **********/

    // user can update the poll if starts_at > now
    // it panics if
    // - user tries to create an invalid poll
    // - if poll aready exists and starts_at < now
    pub fn create_poll(
        &mut self,
        iah_only: bool,
        questions: Vec<PollQuestion>,
        starts_at: u64,
        ends_at: u64,
        title: String,
        tags: Vec<String>,
        description: Option<String>,
        link: Option<String>,
    ) -> PollId {
        let created_at = env::block_timestamp_ms();
        require!(
            created_at < starts_at,
            format!("poll start must be in the future")
        );
        let poll_id = self.next_poll_id;
        self.next_poll_id += 1;
        self.polls.insert(
            &poll_id,
            &Poll {
                iah_only,
                questions,
                starts_at,
                ends_at,
                title,
                tags,
                description,
                link,
                created_at,
            },
        );
        poll_id
    }

    // user can change his answer when the poll is still active.
    // it panics if
    // - poll not found
    // - poll not active
    // - poll.verified_humans_only is true, and user is not verified on IAH
    // - user tries to vote with an invalid answer to a question
    pub fn respond(&mut self, poll_id: PollId, answers: Vec<Option<PollQuestionAnswer>>) {
        // check if poll exists and is active
        self.assert_active(poll_id);
        // if iah calls the registry to verify the iah sbt
        self.assert_human(poll_id);
        let questions = self.polls.get(&poll_id).unwrap().questions;
        let unwrapped_answers = Vec::new();
        for i in 0..questions.len() {
            match answers[i] {
                Some(PollQuestionAnswer::YesNo(value)) => {
                    if value {
                        let results = self.poll_questions_results.get((&poll_id, i));
                    }
                }
                PollQuestionAnswer::TextChoices => {
                    // TODO: implement
                }
                PollQuestionAnswer::PictureChoices(value) => {
                    // TODO: implement
                }
                PollQuestionAnswer::OpinionScale(value) => {
                    // TODO: implement
                }
                PollQuestionAnswer::TextAnswer => {
                    println!("Not supported yet");
                }
            }

            require!(
                questions[i].required && answers[i].is_some(),
                format!("poll question {} requires an answer", i)
            );
            if answers[i].is_some() {
                unwrapped_answers.push(answers[i].unwrap());
            }
        }
        let results = self.results.get(&poll_id).unwrap();
        results.append(unwrapped_answers);
        self.results.insert(poll_id, results);

        // update the results
    }

    pub fn my_responder(&self, poll_id: PollId) -> Vec<Option<PollQuestionAnswer>> {
        unimplemented!();
    }

    // returns None if poll is not found
    pub fn result(poll_id: usize) -> PollResults {
        unimplemented!();
    }

    /**********
     * INTERNAL
     **********/

    fn assert_active(&self, poll_id: PollId) {
        let poll = self.polls.get(&poll_id).expect("poll not found");
        require!(poll.ends_at > env::block_timestamp_ms(), "poll not active");
    }

    fn assert_human(&self, poll_id: PollId) {
        let poll = self.polls.get(&poll_id).unwrap();
        if poll.iah_only {
            // TODO: call registry to verify humanity
        }
    }

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
