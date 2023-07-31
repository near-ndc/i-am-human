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
    pub results: UnorderedMap<PollId, Results>,
    pub answers: UnorderedMap<(PollId, AccountId), Vec<Answer>>,
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
            answers: UnorderedMap::new(StorageKey::Answers),
            registry,
            next_poll_id: 0,
        }
    }

    /**********
     * QUERIES
     **********/

    pub fn my_respond(&self, poll_id: PollId) -> Vec<Answer> {
        let caller = env::predecessor_account_id();
        self.answers
            .get(&(poll_id, caller))
            .expect("respond not found")
    }

    // returns None if poll is not found
    pub fn result(&self, poll_id: usize) -> Results {
        unimplemented!();
    }

    // user can update the poll if starts_at > now
    // it panics if
    // - user tries to create an invalid poll
    // - if poll aready exists and starts_at < now
    pub fn create_poll(
        &mut self,
        iah_only: bool,
        questions: Vec<Question>,
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
    pub fn respond(&mut self, poll_id: PollId, answers: Vec<Option<Answer>>) {
        let caller = env::predecessor_account_id();
        // check if poll exists and is active
        self.assert_active(poll_id);
        // if iah calls the registry to verify the iah sbt
        self.assert_human(poll_id, &caller);
        let questions: Vec<Question> = self.polls.get(&poll_id).unwrap().questions;
        let mut unwrapped_answers: Vec<Answer> = Vec::new();
        let poll_results = self.results.get(&poll_id).unwrap();
        let mut results = poll_results.results;
        for i in 0..questions.len() {
            require!(
                questions[i].required && answers[i].is_some(),
                format!("poll question {} requires an answer", i)
            );

            match (&answers[i], &results[i]) {
                (Some(Answer::YesNo(yes_no)), Result::YesNo((yes, no))) => {
                    if *yes_no {
                        results[i] = Result::YesNo((*yes + 1, *no));
                    } else {
                        results[i] = Result::YesNo((*yes + 1, *no + 1));
                    }
                }
                (Some(Answer::TextChoices(value)), Result::TextChoices(vector)) => {
                    let mut new_vec = Vec::new();
                    for i in value {
                        new_vec[*i] = vector[*i] + *i as u32;
                    }
                    results[i] = Result::TextChoices(new_vec);
                }
                (Some(Answer::PictureChoices(value)), Result::PictureChoices(vector)) => {
                    let mut new_vec = Vec::new();
                    for i in value {
                        new_vec[*i] = vector[*i] + *i as u32;
                    }
                    results[i] = Result::PictureChoices(new_vec);
                }
                (Some(Answer::OpinionScale(value)), Result::OpinionScale(opinion)) => {
                    results.insert(
                        i,
                        Result::OpinionScale(OpinionScaleResult {
                            sum: opinion.sum + *value as u32,
                            num: opinion.num + 1 as u32,
                        }),
                    );
                }
                // (Some(Answer::TextAnswer(answer)), Result::TextAnswer(answers)) => {
                //     let mut new_vec = Vec::new();
                //     new_vec = *answers;
                //     new_vec.push(*answer);
                // }
                (_, _) => env::panic_str("error"),
            }
            if answers[i].is_some() {
                unwrapped_answers.push(answers[i].clone().unwrap());
            }
        }
        let mut answers = self.answers.get(&(poll_id, caller.clone())).unwrap();
        answers.append(&mut unwrapped_answers);
        self.answers.insert(&(poll_id, caller), &answers);
    }

    /**********
     * INTERNAL
     **********/
    fn assert_active(&self, poll_id: PollId) {
        let poll = self.polls.get(&poll_id).expect("poll not found");
        require!(poll.ends_at > env::block_timestamp_ms(), "poll not active");
    }

    fn assert_human(&self, poll_id: PollId, caller: &AccountId) {
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

    use crate::{Answer, Contract, Question};

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

    #[test]
    fn flow1() {
        let question = Question {
            question_type: Answer::YesNo(true),   // required
            required: true, // required, if true users can't vote without having an answer for this question
            title: String::from("Hello, world!"), // required
            description: None, // optional
            image: None,    // optional
            labels: None,   // if applicable, labels for the opinion scale question
            choices: None,  // if applicable, choices for the text and picture choices question
        };
        let tags = vec![String::from("tag1"), String::from("tag2")];
        let (mut ctx, mut ctr) = setup(&admin());
        let poll_id = ctr.create_poll(
            false,
            vec![question],
            1,
            100,
            String::from("Hello, world!"),
            tags,
            None,
            None,
        );
        ctx.predecessor_account_id = alice();
        testing_env!(ctx.clone());
        let answers = vec![Some(Answer::YesNo(true))];
        ctr.respond(poll_id, answers);
    }
}
