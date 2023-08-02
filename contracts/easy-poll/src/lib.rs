use std::fmt::format;
use std::future::Future;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, UnorderedMap};
use near_sdk::{env, near_bindgen, require, AccountId, PanicOnDefault};

pub use crate::errors::PollError;
pub use crate::storage::*;

mod errors;
mod storage;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// Account authorized to add new minting authority
    pub admin: AccountId,
    /// map of classId -> to set of accounts authorized to mint
    pub polls: UnorderedMap<PollId, Poll>,
    pub results: LookupMap<PollId, Results>,
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
            results: LookupMap::new(StorageKey::Results),
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
    pub fn results(&self, poll_id: u64) -> Results {
        self.results.get(&poll_id).expect("poll not found")
    }

    // TODO: limit the max lenght of single answer and based on that return a fixed value of answers
    // Function must be called until true is returned -> meaning all the answers were returned
    // returns None if poll is not found
    // `question` must be an index of the text question in the poll
    pub fn result_answers(
        &self,
        poll_id: usize,
        question: usize,
        from_answer: u64,
    ) -> (Vec<String>, bool) {
        //TODO check if question is type `TextAnswer`
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
        self.initalize_results(poll_id, &questions);
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
    #[handle_result]
    pub fn respond(
        &mut self,
        poll_id: PollId,
        answers: Vec<Option<Answer>>,
    ) -> Result<(), PollError> {
        let caller = env::predecessor_account_id();
        // check if poll exists and is active
        self.assert_active(poll_id);
        self.assert_answered(poll_id, &caller);
        // if iah calls the registry to verify the iah sbt
        self.assert_human(poll_id, &caller);
        let questions: Vec<Question> = self.polls.get(&poll_id).unwrap().questions;
        let mut unwrapped_answers: Vec<Answer> = Vec::new();
        let mut poll_results = self.results.get(&poll_id).unwrap();
        // let mut results = poll_results.results;
        for i in 0..questions.len() {
            if questions[i].required && answers[i].is_none() {
                env::panic_str(format!("poll question {} requires an answer", i).as_str());
            }

            match (&answers[i], &poll_results.results[i]) {
                (Some(Answer::YesNo(yes_no)), PollResult::YesNo((yes, no))) => {
                    if *yes_no {
                        poll_results.results[i] = PollResult::YesNo((*yes + 1, *no));
                    } else {
                        poll_results.results[i] = PollResult::YesNo((*yes, *no + 1));
                    }
                }
                (Some(Answer::TextChoices(value)), PollResult::TextChoices(vector)) => {
                    let mut new_vec = Vec::new();
                    for i in value {
                        new_vec[*i] = vector[*i] + *i as u32;
                    }
                    poll_results.results[i] = PollResult::TextChoices(new_vec);
                }
                (Some(Answer::PictureChoices(value)), PollResult::PictureChoices(vector)) => {
                    let mut new_vec = Vec::new();
                    for i in value {
                        new_vec[*i] = vector[*i] + *i as u32;
                    }
                    poll_results.results[i] = PollResult::PictureChoices(new_vec);
                }
                (Some(Answer::OpinionScale(value)), PollResult::OpinionScale(opinion)) => {
                    if *value > 10 {
                        env::panic_str("opinion must be between 0 and 10");
                    }
                    poll_results.results[i] = PollResult::OpinionScale(OpinionScaleResult {
                        sum: opinion.sum + *value as u32,
                        num: opinion.num + 1 as u32,
                    });
                }
                // (Some(Answer::TextAnswer(answer)), Result::TextAnswer(answers)) => {
                //     let mut new_vec = Vec::new();
                //     new_vec = *answers;
                //     new_vec.push(*answer);
                // }
                (_, _) => (),
            }
            if answers[i].is_some() {
                unwrapped_answers.push(answers[i].clone().unwrap());
            }
        }
        let mut answers = self
            .answers
            .get(&(poll_id, caller.clone()))
            .unwrap_or(Vec::new());
        answers.append(&mut unwrapped_answers);
        self.answers.insert(&(poll_id, caller), &answers);
        // update the status and number of participants
        poll_results.status = Status::Active;
        poll_results.number_of_participants += 1;
        self.results.insert(&poll_id, &poll_results);
        Ok(())
    }

    /**********
     * INTERNAL
     **********/
    fn assert_active(&self, poll_id: PollId) {
        let poll = self.polls.get(&poll_id).expect("poll not found");
        let current_timestamp = env::block_timestamp_ms();
        require!(
            poll.starts_at < current_timestamp,
            format!(
                "poll have not started yet, start_at: {:?}, current_timestamp: {:?}",
                poll.starts_at, current_timestamp
            )
        );
        require!(poll.ends_at > current_timestamp, "poll not active");
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

    fn assert_answered(&self, poll_id: PollId, caller: &AccountId) {
        require!(
            self.answers.get(&(poll_id, caller.clone())).is_none(),
            format!("user: {} has already answered", caller)
        );
    }

    fn initalize_results(&mut self, poll_id: PollId, questions: &Vec<Question>) {
        let mut results = Vec::new();
        for question in questions {
            results.push(match question.question_type {
                Answer::YesNo(_) => PollResult::YesNo((0, 0)),
                Answer::TextChoices(_) => PollResult::TextChoices(Vec::new()),
                Answer::PictureChoices(_) => PollResult::PictureChoices(Vec::new()),
                Answer::OpinionScale(_) => {
                    PollResult::OpinionScale(OpinionScaleResult { sum: 0, num: 0 })
                }
                Answer::TextAnswer(_) => PollResult::TextAnswer(Vec::new()),
            });
        }
        self.results.insert(
            &poll_id,
            &Results {
                status: Status::NotStarted,
                number_of_participants: 0,
                results: results,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use near_sdk::{test_utils::VMContextBuilder, testing_env, AccountId, Balance, VMContext};
    use sbt::{ClassId, ContractMetadata, TokenMetadata};

    use crate::{Answer, Contract, OpinionScaleResult, PollResult, Question, Results, Status};

    const MILI_SECOND: u64 = 1000000; // nanoseconds

    fn alice() -> AccountId {
        AccountId::new_unchecked("alice.near".to_string())
    }

    fn bob() -> AccountId {
        AccountId::new_unchecked("bob.near".to_string())
    }

    fn charlie() -> AccountId {
        AccountId::new_unchecked("charlie.near".to_string())
    }

    fn registry() -> AccountId {
        AccountId::new_unchecked("registry.near".to_string())
    }

    fn admin() -> AccountId {
        AccountId::new_unchecked("admin.near".to_string())
    }

    fn questions() -> Vec<Question> {
        let mut questions = Vec::new();
        questions.push(Question {
            question_type: Answer::YesNo(true),
            required: false,
            title: String::from("Yes and no test!"),
            description: None,
            image: None,
            labels: None,
            choices: None,
        });
        questions.push(Question {
            question_type: Answer::TextChoices(vec![0, 0, 0]),
            required: false,
            title: String::from("Yes and no test!"),
            description: None,
            image: None,
            labels: None,
            choices: Some(vec![
                String::from("agree"),
                String::from("disagree"),
                String::from("no opinion"),
            ]),
        });
        questions.push(Question {
            question_type: Answer::OpinionScale(0),
            required: false,
            title: String::from("Opinion test!"),
            description: None,
            image: None,
            labels: None,
            choices: None,
        });
        questions
    }

    fn setup(predecessor: &AccountId) -> (VMContext, Contract) {
        let mut ctx = VMContextBuilder::new()
            .predecessor_account_id(admin())
            .block_timestamp(MILI_SECOND)
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
    fn yes_no_flow() {
        let tags = vec![String::from("tag1"), String::from("tag2")];
        let (mut ctx, mut ctr) = setup(&admin());
        let poll_id = ctr.create_poll(
            false,
            questions(),
            2,
            100,
            String::from("Hello, world!"),
            tags,
            None,
            None,
        );
        ctx.predecessor_account_id = alice();
        ctx.block_timestamp = MILI_SECOND * 3;
        testing_env!(ctx.clone());
        ctr.respond(poll_id, vec![Some(Answer::YesNo(true)), None, None]);
        ctx.predecessor_account_id = bob();
        testing_env!(ctx.clone());
        ctr.respond(poll_id, vec![Some(Answer::YesNo(true)), None, None]);
        ctx.predecessor_account_id = charlie();
        testing_env!(ctx.clone());
        ctr.respond(poll_id, vec![Some(Answer::YesNo(false)), None, None]);
        let results = ctr.results(poll_id);
        assert_eq!(
            results,
            Results {
                status: Status::Active,
                number_of_participants: 3,
                results: vec![
                    PollResult::YesNo((2, 1)),
                    PollResult::TextChoices(vec![]),
                    PollResult::OpinionScale(OpinionScaleResult { sum: 0, num: 0 })
                ]
            }
        )
    }

    #[test]
    fn opinion_scale_flow() {
        let tags = vec![String::from("tag1"), String::from("tag2")];
        let (mut ctx, mut ctr) = setup(&admin());
        let poll_id = ctr.create_poll(
            false,
            questions(),
            2,
            100,
            String::from("Multiple questions test!"),
            tags,
            None,
            None,
        );
        ctx.predecessor_account_id = alice();
        ctx.block_timestamp = (MILI_SECOND * 3);
        testing_env!(ctx.clone());
        ctr.respond(
            poll_id,
            vec![
                Some(Answer::YesNo(true)),
                None,
                Some(Answer::OpinionScale(5)),
            ],
        );
        ctx.predecessor_account_id = bob();
        testing_env!(ctx.clone());
        ctr.respond(poll_id, vec![None, None, Some(Answer::OpinionScale(10))]);
        ctx.predecessor_account_id = charlie();
        testing_env!(ctx.clone());
        ctr.respond(poll_id, vec![None, None, Some(Answer::OpinionScale(2))]);
        let results = ctr.results(poll_id);
        assert_eq!(
            results,
            Results {
                status: Status::Active,
                number_of_participants: 3,
                results: vec![
                    PollResult::YesNo((1, 0)),
                    PollResult::TextChoices(vec![]),
                    PollResult::OpinionScale(OpinionScaleResult { sum: 17, num: 3 })
                ]
            }
        )
    }
}
