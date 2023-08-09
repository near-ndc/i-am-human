use ext::ext_registry;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, UnorderedMap};
use near_sdk::{env, near_bindgen, require, AccountId, PanicOnDefault};

pub use crate::constants::*;
pub use crate::errors::PollError;
pub use crate::ext::*;
pub use crate::storage::*;

mod constants;
mod errors;
mod ext;
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

    /// returns None if poll is not found
    /// this should result all the restuls but `TextAnswers`
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

        match self.assert_active(poll_id) {
            Err(err) => return Err(err),
            Ok(_) => (),
        };

        // TODO: I think we should add a option for the poll creator to choose whether changing
        // the answers while the poll is active is allowed or not
        self.assert_answered(poll_id, &caller);
        let poll = self.polls.get(&poll_id).unwrap();
        // if iah calls the registry to verify the iah sbt
        if poll.iah_only {
            ext_registry::ext(self.registry.clone())
                .is_human(caller.clone())
                .then(
                    Self::ext(env::current_account_id())
                        .with_static_gas(GAS_UPVOTE)
                        .on_human_verifed(true, caller, poll_id, answers),
                );
        } else {
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_UPVOTE)
                .on_human_verifed(false, caller, poll_id, answers);
        }
        Ok(())
    }

    /**********
     * PRIVATE
     **********/

    #[private]
    #[handle_result]
    pub fn on_human_verifed(
        &mut self,
        #[callback_unwrap] tokens: Vec<(AccountId, Vec<sbt::TokenId>)>,
        iah_only: bool,
        caller: AccountId,
        poll_id: PollId,
        answers: Vec<Option<Answer>>,
    ) -> Result<(), PollError> {
        if iah_only && tokens.is_empty() {
            return Err(PollError::NoSBTs);
        }
        let questions: Vec<Question> = self.polls.get(&poll_id).unwrap().questions;
        let mut unwrapped_answers: Vec<Answer> = Vec::new();
        let mut poll_results = self.results.get(&poll_id).unwrap();
        // let mut results = poll_results.results;
        for i in 0..questions.len() {
            if questions[i].required && answers[i].is_none() {
                return Err(PollError::RequiredAnswer);
            }

            match (&answers[i], &poll_results.results[i]) {
                (Some(Answer::YesNo(answer)), PollResult::YesNo((yes, no))) => {
                    if *answer {
                        poll_results.results[i] = PollResult::YesNo((*yes + 1, *no));
                    } else {
                        poll_results.results[i] = PollResult::YesNo((*yes, *no + 1));
                    }
                }
                (Some(Answer::TextChoices(answer)), PollResult::TextChoices(results)) => {
                    let mut res: Vec<u32> = results.to_vec();
                    for i in 0..answer.len() {
                        if answer[i] == true {
                            res[i] += 1;
                        }
                    }
                    poll_results.results[i] = PollResult::TextChoices(res);
                }
                (Some(Answer::PictureChoices(answer)), PollResult::PictureChoices(results)) => {
                    let mut res: Vec<u32> = results.to_vec();
                    for i in 0..answer.len() {
                        if answer[i] == true {
                            res[i] += 1;
                        }
                    }
                    poll_results.results[i] = PollResult::PictureChoices(res);
                }
                (Some(Answer::OpinionScale(answer)), PollResult::OpinionScale(results)) => {
                    if *answer > 10 {
                        return Err(PollError::OpinionScale);
                    }
                    poll_results.results[i] = PollResult::OpinionScale(OpinionScaleResult {
                        sum: results.sum + *answer as u32,
                        num: results.num + 1 as u32,
                    });
                }
                (Some(Answer::TextAnswer(answer)), PollResult::TextAnswer(results)) => {
                    let mut results = results.clone();
                    results.push(answer.clone());
                    poll_results.results[i] = PollResult::TextAnswer(results);
                }
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
    #[handle_result]
    fn assert_active(&self, poll_id: PollId) -> Result<(), PollError> {
        let poll = match self.polls.get(&poll_id) {
            Some(poll) => poll,
            None => return Err(PollError::NotFound),
        };
        let current_timestamp = env::block_timestamp_ms();
        if poll.starts_at < current_timestamp || poll.ends_at > current_timestamp {
            return Err(PollError::NotActive);
        }
        Ok(())
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
                Answer::TextChoices(_) => {
                    PollResult::TextChoices(vec![0; question.choices.clone().unwrap().len()])
                }
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
    use near_sdk::{test_utils::VMContextBuilder, testing_env, AccountId, VMContext};

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
            max_choices: None,
        });
        questions.push(Question {
            question_type: Answer::TextChoices(vec![false, false, false]),
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
            max_choices: Some(1),
        });
        questions.push(Question {
            question_type: Answer::OpinionScale(0),
            required: false,
            title: String::from("Opinion test!"),
            description: None,
            image: None,
            labels: None,
            choices: None,
            max_choices: None,
        });
        questions.push(Question {
            question_type: Answer::TextAnswer(String::from("")),
            required: false,
            title: String::from("Opinion test!"),
            description: None,
            image: None,
            labels: None,
            choices: None,
            max_choices: None,
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
        let ctr = Contract::new(admin(), registry());
        ctx.predecessor_account_id = predecessor.clone();
        testing_env!(ctx.clone());
        return (ctx, ctr);
    }

    #[test]
    fn assert_admin() {
        let (_, ctr) = setup(&admin());
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
        let mut res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![Some(Answer::YesNo(true)), None, None, None],
        );
        assert!(res.is_ok());
        ctx.predecessor_account_id = bob();
        testing_env!(ctx.clone());
        res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![Some(Answer::YesNo(true)), None, None, None],
        );
        assert!(res.is_ok());
        ctx.predecessor_account_id = charlie();
        testing_env!(ctx.clone());
        res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![Some(Answer::YesNo(false)), None, None, None],
        );
        assert!(res.is_ok());
        let results = ctr.results(poll_id);
        assert_eq!(
            results,
            Results {
                status: Status::Active,
                number_of_participants: 3,
                results: vec![
                    PollResult::YesNo((2, 1)),
                    PollResult::TextChoices(vec![0, 0, 0]),
                    PollResult::OpinionScale(OpinionScaleResult { sum: 0, num: 0 }),
                    PollResult::TextAnswer(vec![])
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
        ctx.block_timestamp = MILI_SECOND * 3;
        testing_env!(ctx.clone());
        let mut res = ctr.on_human_verifed(
            vec![],
            false,
            alice(),
            poll_id,
            vec![
                Some(Answer::YesNo(true)),
                None,
                Some(Answer::OpinionScale(5)),
                None,
            ],
        );
        assert!(res.is_ok());
        ctx.predecessor_account_id = bob();
        testing_env!(ctx.clone());
        res = ctr.on_human_verifed(
            vec![],
            false,
            bob(),
            poll_id,
            vec![None, None, Some(Answer::OpinionScale(10)), None],
        );
        assert!(res.is_ok());
        ctx.predecessor_account_id = charlie();
        testing_env!(ctx.clone());
        res = ctr.on_human_verifed(
            vec![],
            false,
            charlie(),
            poll_id,
            vec![None, None, Some(Answer::OpinionScale(2)), None],
        );
        assert!(res.is_ok());
        let results = ctr.results(poll_id);
        assert_eq!(
            results,
            Results {
                status: Status::Active,
                number_of_participants: 3,
                results: vec![
                    PollResult::YesNo((1, 0)),
                    PollResult::TextChoices(vec![0, 0, 0]),
                    PollResult::OpinionScale(OpinionScaleResult { sum: 17, num: 3 }),
                    PollResult::TextAnswer(vec![])
                ]
            }
        )
    }
    #[test]
    fn text_chocies_flow() {
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
        let mut res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![
                None,
                Some(Answer::TextChoices(vec![true, false, false])),
                None,
                None,
            ],
        );
        assert!(res.is_ok());
        ctx.predecessor_account_id = bob();
        testing_env!(ctx.clone());
        res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![
                None,
                Some(Answer::TextChoices(vec![true, false, false])),
                None,
                None,
            ],
        );
        assert!(res.is_ok());
        ctx.predecessor_account_id = charlie();
        testing_env!(ctx.clone());
        res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![
                None,
                Some(Answer::TextChoices(vec![false, true, false])),
                None,
                None,
            ],
        );
        assert!(res.is_ok());
        let results = ctr.results(poll_id);
        assert_eq!(
            results,
            Results {
                status: Status::Active,
                number_of_participants: 3,
                results: vec![
                    PollResult::YesNo((0, 0)),
                    PollResult::TextChoices(vec![2, 1, 0]),
                    PollResult::OpinionScale(OpinionScaleResult { sum: 0, num: 0 }),
                    PollResult::TextAnswer(vec![])
                ]
            }
        )
    }

    #[test]
    fn text_answers_flow() {
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
        let answer1: String = "Answer 1".to_string();
        let answer2: String = "Answer 2".to_string();
        let answer3: String = "Answer 3".to_string();
        let mut res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![None, None, None, Some(Answer::TextAnswer(answer1.clone()))],
        );
        assert!(res.is_ok());
        ctx.predecessor_account_id = bob();
        testing_env!(ctx.clone());
        res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![None, None, None, Some(Answer::TextAnswer(answer2.clone()))],
        );
        assert!(res.is_ok());
        ctx.predecessor_account_id = charlie();
        testing_env!(ctx.clone());
        res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![None, None, None, Some(Answer::TextAnswer(answer3.clone()))],
        );
        assert!(res.is_ok());
        let results = ctr.results(poll_id);
        assert_eq!(
            results,
            Results {
                status: Status::Active,
                number_of_participants: 3,
                results: vec![
                    PollResult::YesNo((0, 0)),
                    PollResult::TextChoices(vec![0, 0, 0]),
                    PollResult::OpinionScale(OpinionScaleResult { sum: 0, num: 0 }),
                    PollResult::TextAnswer(vec![answer1, answer2, answer3])
                ]
            }
        )
    }
}
