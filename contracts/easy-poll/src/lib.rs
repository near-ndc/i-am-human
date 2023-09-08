pub use crate::errors::PollError;
use crate::events::emit_create_poll;
use crate::events::emit_respond;
pub use crate::ext::*;
pub use crate::storage::*;
use cost::MILI_NEAR;
use ext::ext_registry;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, UnorderedMap, Vector};
use near_sdk::{env, near_bindgen, require, AccountId, PanicOnDefault};
use near_sdk::{Balance, Gas};

mod errors;
mod events;
mod ext;
mod storage;

pub const RESPOND_COST: Balance = MILI_NEAR;
pub const RESPOND_CALLBACK_GAS: Gas = Gas(2 * Gas::ONE_TERA.0);

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// map of all polls
    pub polls: UnorderedMap<PollId, Poll>,
    /// map of all results summarized
    pub results: LookupMap<PollId, Results>,
    /// map of all answers, (poll, user) -> vec of answers
    pub answers: UnorderedMap<(PollId, AccountId), Vec<Option<Answer>>>,
    /// text answers are stored in a separate map
    pub text_answers: LookupMap<(PollId, usize), Vector<String>>,
    /// SBT registry.
    pub registry: AccountId,
    /// next poll id
    pub next_poll_id: PollId,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(registry: AccountId) -> Self {
        Self {
            polls: UnorderedMap::new(StorageKey::Polls),
            results: LookupMap::new(StorageKey::Results),
            answers: UnorderedMap::new(StorageKey::Answers),
            text_answers: LookupMap::new(StorageKey::TextAnswers),
            registry,
            next_poll_id: 1,
        }
    }

    /**********
     * QUERIES
     **********/

    /// Returns caller response to the specified poll
    pub fn my_response(&self, poll_id: PollId) -> Vec<Option<Answer>> {
        let caller = env::predecessor_account_id();
        self.answers
            .get(&(poll_id, caller))
            .expect("respond not found")
    }

    /// Returns poll results (except for text answers), if poll not found panics
    pub fn results(&self, poll_id: u64) -> Results {
        self.results.get(&poll_id).expect("poll not found")
    }

    /// Returns text answers in rounds. Starts from the question id provided. Needs to be called until true is returned.
    pub fn result_text_answers(
        &self,
        poll_id: u64,
        question: usize,
        from_answer: usize,
    ) -> (bool, Vec<String>) {
        // We cannot return more than 20 due to gas limit per txn.
        self._result_text_answers(poll_id, question, from_answer, 20)
    }

    /// Returns a fixed value of answers
    // Function must be called until true is returned -> meaning all the answers were returned
    // `question` must be an index of the text question in the poll
    pub fn _result_text_answers(
        &self,
        poll_id: u64,
        question: usize,
        from_answer: usize,
        limit: usize,
    ) -> (bool, Vec<String>) {
        self.polls
            .get(&poll_id)
            .expect("poll not found")
            .questions
            .get(question)
            .expect("question not found");
        let text_answers = self
            .text_answers
            .get(&(poll_id, question))
            .expect("question not type `TextAnswer`");
        let to_return;
        let mut finished = false;
        if from_answer + limit > text_answers.len() as usize {
            to_return = text_answers.to_vec()[from_answer..].to_vec();
            finished = true;
        } else {
            to_return = text_answers.to_vec()[from_answer..from_answer + limit].to_vec();
        }
        (finished, to_return)
    }

    /**********
     * TRANSACTIONS
     **********/

    /// User can update the poll if starts_at > now
    /// it panics if
    /// - user tries to create an invalid poll
    /// - if poll aready exists and starts_at < now
    /// emits create_poll event
    pub fn create_poll(
        &mut self,
        iah_only: bool,
        questions: Vec<Question>,
        starts_at: u64,
        ends_at: u64,
        title: String,
        tags: Vec<String>,
        description: String,
        link: String,
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
        emit_create_poll(poll_id);
        poll_id
    }

    /// user can change his answer when the poll is still active
    // TODO: currently we do not allow users to change the answer
    /// it panics if
    /// - poll not found
    /// - poll not active
    /// - poll.verified_humans_only is true, and user is not verified on IAH
    /// - user tries to vote with an invalid answer to a question
    /// emits repond event
    #[payable]
    #[handle_result]
    pub fn respond(
        &mut self,
        poll_id: PollId,
        answers: Vec<Option<Answer>>,
    ) -> Result<(), PollError> {
        require!(
            env::attached_deposit() >= RESPOND_COST,
            "attached_deposit not sufficient"
        );
        let caller = env::predecessor_account_id();

        match self.assert_active(poll_id) {
            Err(err) => return Err(err),
            Ok(_) => (),
        };

        // TODO: I think we should add a option for the poll creator to choose whether changing
        // the answers while the poll is active is allowed or not
        self.assert_answered(poll_id, &caller);
        let poll = match self.polls.get(&poll_id) {
            None => return Err(PollError::NotFound),
            Some(poll) => poll,
        };
        // if iah calls the registry to verify the iah sbt
        if poll.iah_only {
            ext_registry::ext(self.registry.clone())
                .is_human(caller.clone())
                .then(
                    Self::ext(env::current_account_id())
                        .with_static_gas(RESPOND_CALLBACK_GAS)
                        .on_human_verifed(true, caller, poll_id, answers),
                );
        } else {
            Self::ext(env::current_account_id())
                .with_static_gas(RESPOND_CALLBACK_GAS)
                .on_human_verifed(false, caller, poll_id, answers);
        }
        Ok(())
    }

    /**********
     * PRIVATE
     **********/

    /// Callback for the respond method.
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
        let questions: Vec<Question> = self.polls.get(&poll_id).expect("poll not found").questions;
        if questions.len() != answers.len() {
            return Err(PollError::IncorrectAnswerVector);
        }
        let mut unwrapped_answers: Vec<Option<Answer>> = Vec::new();
        let mut poll_results = self.results.get(&poll_id).expect("results not found");

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
                (Some(Answer::OpinionRange(answer)), PollResult::OpinionRange(results)) => {
                    if *answer > 10 {
                        return Err(PollError::OpinionRange);
                    }
                    poll_results.results[i] = PollResult::OpinionRange(OpinionRangeResult {
                        sum: results.sum + *answer as u64,
                        num: results.num + 1 as u64,
                    });
                }
                (Some(Answer::TextAnswer(answer)), _) => {
                    let mut answers = self
                        .text_answers
                        .get(&(poll_id, i))
                        .expect(&format!("question not found for index {:?}", i));
                    answers.push(answer);
                    self.text_answers.insert(&(poll_id, i), &answers);
                }
                (None, _) => {
                    unwrapped_answers.push(None);
                }
                (_, _) => return Err(PollError::WrongAnswer),
            }
            if answers[i].is_some() {
                unwrapped_answers.push(Some(answers[i].clone().unwrap()));
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
        poll_results.participants += 1;
        self.results.insert(&poll_id, &poll_results);
        emit_respond(poll_id);
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
        if poll.starts_at > current_timestamp || poll.ends_at < current_timestamp {
            return Err(PollError::NotActive);
        }
        Ok(())
    }

    fn assert_answered(&self, poll_id: PollId, caller: &AccountId) {
        require!(
            self.answers.get(&(poll_id, caller.clone())).is_none(),
            format!("user: {} has already answered", caller)
        );
    }

    fn initalize_results(&mut self, poll_id: PollId, questions: &Vec<Question>) {
        let mut results = Vec::new();
        let mut index = 0;
        for question in questions {
            match question.question_type {
                Answer::YesNo(_) => results.push(PollResult::YesNo((0, 0))),
                Answer::TextChoices(_) => results.push(PollResult::TextChoices(vec![
                    0;
                    question
                        .choices
                        .clone()
                        .unwrap()
                        .len()
                ])),
                Answer::PictureChoices(_) => results.push(PollResult::PictureChoices(Vec::new())),
                Answer::OpinionRange(_) => {
                    results.push(PollResult::OpinionRange(OpinionRangeResult {
                        sum: 0,
                        num: 0,
                    }))
                }
                Answer::TextAnswer(_) => {
                    results.push(PollResult::TextAnswer);
                    self.text_answers
                        .insert(&(poll_id, index), &Vector::new(StorageKey::TextAnswers));
                }
            };
            index += 1;
        }
        self.results.insert(
            &poll_id,
            &Results {
                status: Status::NotStarted,
                participants: 0,
                results: results,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use near_sdk::{
        test_utils::{self, VMContextBuilder},
        testing_env, AccountId, VMContext,
    };

    use crate::{
        Answer, Contract, OpinionRangeResult, PollError, PollId, PollResult, Question, Results,
        Status, RESPOND_COST,
    };

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

    fn tags() -> Vec<String> {
        vec![String::from("tag1"), String::from("tag2")]
    }

    fn question_text_answers(required: bool) -> Question {
        Question {
            question_type: Answer::TextAnswer(String::from("")),
            required,
            title: String::from("Opinion test!"),
            description: None,
            image: None,
            labels: None,
            choices: None,
            max_choices: None,
        }
    }

    fn question_yes_no(required: bool) -> Question {
        Question {
            question_type: Answer::YesNo(true),
            required,
            title: String::from("Yes and no test!"),
            description: None,
            image: None,
            labels: None,
            choices: None,
            max_choices: None,
        }
    }

    fn question_text_choices(required: bool) -> Question {
        Question {
            question_type: Answer::TextChoices(vec![false, false, false]),
            required,
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
        }
    }

    fn question_opinion_range(required: bool) -> Question {
        Question {
            question_type: Answer::OpinionRange(0),
            required,
            title: String::from("Opinion test!"),
            description: None,
            image: None,
            labels: None,
            choices: None,
            max_choices: None,
        }
    }

    fn mk_batch_text_answers(
        ctr: &mut Contract,
        predecessor: AccountId,
        poll_id: PollId,
        num_answers: u64,
    ) {
        for i in 0..num_answers {
            let res = ctr.on_human_verifed(
                vec![],
                false,
                predecessor.clone(),
                poll_id,
                vec![Some(Answer::TextAnswer(format!(
                    "Answer Answer Answer Answer Answer Answer Answer Answer Answer{}",
                    i
                )))],
            );
            assert!(res.is_ok());
        }
    }

    fn setup(predecessor: &AccountId) -> (VMContext, Contract) {
        let mut ctx = VMContextBuilder::new()
            .predecessor_account_id(alice())
            .block_timestamp(MILI_SECOND)
            .is_view(false)
            .build();
        testing_env!(ctx.clone());
        let ctr = Contract::new(registry());
        ctx.predecessor_account_id = predecessor.clone();
        testing_env!(ctx.clone());
        return (ctx, ctr);
    }

    #[test]
    #[should_panic(expected = "poll start must be in the future")]
    fn create_poll_wrong_time() {
        let (_, mut ctr) = setup(&alice());
        ctr.create_poll(
            false,
            vec![question_yes_no(true)],
            1,
            100,
            String::from("Hello, world!"),
            tags(),
            String::from(""),
            String::from(""),
        );
    }

    #[test]
    fn create_poll() {
        let (_, mut ctr) = setup(&alice());
        ctr.create_poll(
            false,
            vec![question_yes_no(true)],
            2,
            100,
            String::from("Hello, world!"),
            tags(),
            String::from(""),
            String::from(""),
        );
        let expected_event = r#"EVENT_JSON:{"standard":"ndc-easy-polls","version":"0.0.1","event":"create_poll","data":{"poll_id":1}}"#;
        assert!(test_utils::get_logs().len() == 1);
        assert_eq!(test_utils::get_logs()[0], expected_event);
    }

    #[test]
    #[should_panic(expected = "respond not found")]
    fn my_response_not_found() {
        let (_, mut ctr) = setup(&alice());
        let poll_id = ctr.create_poll(
            false,
            vec![question_yes_no(true)],
            2,
            100,
            String::from("Hello, world!"),
            tags(),
            String::from(""),
            String::from(""),
        );
        ctr.my_response(poll_id);
    }

    #[test]
    fn my_response() {
        let (mut ctx, mut ctr) = setup(&alice());
        let poll_id = ctr.create_poll(
            false,
            vec![question_yes_no(false), question_yes_no(true)],
            2,
            100,
            String::from("Hello, world!"),
            tags(),
            String::from(""),
            String::from(""),
        );
        ctx.block_timestamp = MILI_SECOND * 3;
        testing_env!(ctx.clone());
        let res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![None, Some(Answer::YesNo(true))],
        );
        assert!(res.is_ok());
        let res = ctr.my_response(poll_id);
        assert_eq!(res, vec![None, Some(Answer::YesNo(true))])
    }

    #[test]
    #[should_panic(expected = "poll not found")]
    fn results_poll_not_found() {
        let (_, ctr) = setup(&alice());
        ctr.results(1);
    }

    #[test]
    fn results() {
        let (_, mut ctr) = setup(&alice());
        let poll_id = ctr.create_poll(
            false,
            vec![question_yes_no(true)],
            2,
            100,
            String::from("Hello, world!"),
            tags(),
            String::from(""),
            String::from(""),
        );
        let res = ctr.results(poll_id);
        let expected = Results {
            status: Status::NotStarted,
            participants: 0,
            results: vec![PollResult::YesNo((0, 0))],
        };
        assert_eq!(res, expected);
    }

    #[test]
    #[should_panic(expected = "poll not found")]
    fn result_text_answers_poll_not_found() {
        let (_, ctr) = setup(&alice());
        ctr.result_text_answers(0, 0, 0);
    }

    #[test]
    #[should_panic(expected = "question not found")]
    fn result_text_answers_wrong_question() {
        let (_, mut ctr) = setup(&alice());
        let poll_id = ctr.create_poll(
            false,
            vec![question_yes_no(true)],
            2,
            100,
            String::from("Hello, world!"),
            tags(),
            String::from(""),
            String::from(""),
        );
        ctr.result_text_answers(poll_id, 1, 0);
    }

    #[test]
    #[should_panic(expected = "question not type `TextAnswer`")]
    fn result_text_answers_wrong_type() {
        let (_, mut ctr) = setup(&alice());
        let poll_id = ctr.create_poll(
            false,
            vec![question_yes_no(true)],
            2,
            100,
            String::from("Hello, world!"),
            tags(),
            String::from(""),
            String::from(""),
        );
        ctr.result_text_answers(poll_id, 0, 0);
    }

    #[test]
    #[should_panic(expected = "attached_deposit not sufficient")]
    fn respond_wrong_deposit() {
        let (mut ctx, mut ctr) = setup(&alice());
        ctx.attached_deposit = RESPOND_COST - 1;
        testing_env!(ctx);
        let res = ctr.respond(0, vec![Some(Answer::YesNo(true))]);
        assert!(res.is_err());
    }

    #[test]
    fn respond_poll_not_active() {
        let (mut ctx, mut ctr) = setup(&alice());
        let poll_id = ctr.create_poll(
            false,
            vec![question_yes_no(true)],
            2,
            100,
            String::from("Hello, world!"),
            tags(),
            String::from(""),
            String::from(""),
        );
        ctx.attached_deposit = RESPOND_COST;
        testing_env!(ctx.clone());
        // too early
        match ctr.respond(poll_id, vec![Some(Answer::YesNo(true))]) {
            Err(err) => {
                println!("Received error: {:?}", err);
                match err {
                    PollError::NotActive => println!("Expected error: PollError::NotActive"),
                    _ => panic!("Unexpected error: {:?}", err),
                }
            }
            Ok(_) => panic!("Received Ok result, but expected an error"),
        }
        ctx.block_timestamp = MILI_SECOND * 101;
        testing_env!(ctx);
        // too late
        match ctr.respond(poll_id, vec![Some(Answer::YesNo(true))]) {
            Err(err) => {
                println!("Received error: {:?}", err);
                match err {
                    PollError::NotActive => println!("Expected error: PollError::NotActive"),
                    _ => panic!("Unexpected error: {:?}", err),
                }
            }
            Ok(_) => panic!("Received Ok result, but expected an error"),
        }
    }

    #[test]
    fn yes_no_flow() {
        let (mut ctx, mut ctr) = setup(&alice());
        let poll_id = ctr.create_poll(
            false,
            vec![question_yes_no(true)],
            2,
            100,
            String::from("Hello, world!"),
            tags(),
            String::from(""),
            String::from(""),
        );
        ctx.block_timestamp = MILI_SECOND * 3;
        testing_env!(ctx.clone());
        let mut res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![Some(Answer::YesNo(true))],
        );
        assert!(res.is_ok());

        let expected_event = r#"EVENT_JSON:{"standard":"ndc-easy-polls","version":"0.0.1","event":"respond","data":{"poll_id":1}}"#;
        assert!(test_utils::get_logs().len() == 1);
        assert_eq!(test_utils::get_logs()[0], expected_event);

        ctx.predecessor_account_id = bob();
        testing_env!(ctx.clone());
        res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![Some(Answer::YesNo(true))],
        );
        assert!(res.is_ok());

        assert!(test_utils::get_logs().len() == 1);
        assert_eq!(test_utils::get_logs()[0], expected_event);

        ctx.predecessor_account_id = charlie();
        testing_env!(ctx.clone());
        res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![Some(Answer::YesNo(false))],
        );
        assert!(res.is_ok());

        assert!(test_utils::get_logs().len() == 1);
        assert_eq!(test_utils::get_logs()[0], expected_event);

        let results = ctr.results(poll_id);
        assert_eq!(
            results,
            Results {
                status: Status::Active,
                participants: 3,
                results: vec![PollResult::YesNo((2, 1)),]
            }
        )
    }

    #[test]
    fn opinion_range_out_of_range() {
        let (mut ctx, mut ctr) = setup(&alice());
        let poll_id = ctr.create_poll(
            false,
            vec![question_opinion_range(false)],
            2,
            100,
            String::from("Multiple questions test!"),
            tags(),
            String::from(""),
            String::from(""),
        );
        ctx.block_timestamp = MILI_SECOND * 3;
        testing_env!(ctx);
        match ctr.on_human_verifed(
            vec![],
            false,
            alice(),
            poll_id,
            vec![Some(Answer::OpinionRange(11))],
        ) {
            Err(err) => {
                println!("Received error: {:?}", err);
                match err {
                    PollError::OpinionRange => println!("Expected error: PollError::OpinionRange"),
                    _ => panic!("Unexpected error: {:?}", err),
                }
            }
            Ok(_) => panic!("Received Ok result, but expected an error"),
        }
    }

    #[test]
    fn respond_wrong_answer_vector() {
        let (mut ctx, mut ctr) = setup(&alice());
        let poll_id = ctr.create_poll(
            false,
            vec![question_opinion_range(false)],
            2,
            100,
            String::from("Multiple questions test!"),
            tags(),
            String::from(""),
            String::from(""),
        );
        ctx.block_timestamp = MILI_SECOND * 3;
        testing_env!(ctx);
        match ctr.on_human_verifed(
            vec![],
            false,
            alice(),
            poll_id,
            vec![
                Some(Answer::OpinionRange(10)),
                Some(Answer::OpinionRange(10)),
            ],
        ) {
            Err(err) => {
                println!("Received error: {:?}", err);
                match err {
                    PollError::IncorrectAnswerVector => {
                        println!("Expected error: PollError::IncorrectAnswerVector")
                    }
                    _ => panic!("Unexpected error: {:?}", err),
                }
            }
            Ok(_) => panic!("Received Ok result, but expected an error"),
        }
    }

    #[test]
    fn opinion_range_flow() {
        let (mut ctx, mut ctr) = setup(&alice());
        let poll_id = ctr.create_poll(
            false,
            vec![question_opinion_range(false)],
            2,
            100,
            String::from("Multiple questions test!"),
            tags(),
            String::from(""),
            String::from(""),
        );
        ctx.predecessor_account_id = alice();
        ctx.block_timestamp = MILI_SECOND * 3;
        testing_env!(ctx.clone());
        let mut res = ctr.on_human_verifed(
            vec![],
            false,
            alice(),
            poll_id,
            vec![Some(Answer::OpinionRange(5))],
        );
        assert!(res.is_ok());
        ctx.predecessor_account_id = bob();
        testing_env!(ctx.clone());
        res = ctr.on_human_verifed(
            vec![],
            false,
            bob(),
            poll_id,
            vec![Some(Answer::OpinionRange(10))],
        );
        assert!(res.is_ok());
        ctx.predecessor_account_id = charlie();
        testing_env!(ctx.clone());
        res = ctr.on_human_verifed(
            vec![],
            false,
            charlie(),
            poll_id,
            vec![Some(Answer::OpinionRange(2))],
        );
        assert!(res.is_ok());
        let results = ctr.results(poll_id);
        assert_eq!(
            results,
            Results {
                status: Status::Active,
                participants: 3,
                results: vec![PollResult::OpinionRange(OpinionRangeResult {
                    sum: 17,
                    num: 3
                }),]
            }
        )
    }
    #[test]
    fn text_chocies_flow() {
        let (mut ctx, mut ctr) = setup(&alice());
        let poll_id = ctr.create_poll(
            false,
            vec![question_text_choices(true)],
            2,
            100,
            String::from("Hello, world!"),
            tags(),
            String::from(""),
            String::from(""),
        );
        ctx.predecessor_account_id = alice();
        ctx.block_timestamp = MILI_SECOND * 3;
        testing_env!(ctx.clone());
        let mut res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![Some(Answer::TextChoices(vec![true, false, false]))],
        );
        assert!(res.is_ok());
        ctx.predecessor_account_id = bob();
        testing_env!(ctx.clone());
        res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![Some(Answer::TextChoices(vec![true, false, false]))],
        );
        assert!(res.is_ok());
        ctx.predecessor_account_id = charlie();
        testing_env!(ctx.clone());
        res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![Some(Answer::TextChoices(vec![false, true, false]))],
        );
        assert!(res.is_ok());
        let results = ctr.results(poll_id);
        assert_eq!(
            results,
            Results {
                status: Status::Active,
                participants: 3,
                results: vec![PollResult::TextChoices(vec![2, 1, 0]),]
            }
        )
    }

    #[test]
    fn text_answers_flow() {
        let (mut ctx, mut ctr) = setup(&alice());
        let poll_id = ctr.create_poll(
            false,
            vec![question_text_answers(true)],
            2,
            100,
            String::from("Hello, world!"),
            tags(),
            String::from(""),
            String::from(""),
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
            vec![Some(Answer::TextAnswer(answer1.clone()))],
        );
        assert!(res.is_ok());
        ctx.predecessor_account_id = bob();
        testing_env!(ctx.clone());
        res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![Some(Answer::TextAnswer(answer2.clone()))],
        );
        assert!(res.is_ok());
        ctx.predecessor_account_id = charlie();
        testing_env!(ctx.clone());
        res = ctr.on_human_verifed(
            vec![],
            false,
            ctx.predecessor_account_id,
            poll_id,
            vec![Some(Answer::TextAnswer(answer3.clone()))],
        );
        assert!(res.is_ok());
        let results = ctr.results(poll_id);
        assert_eq!(
            results,
            Results {
                status: Status::Active,
                participants: 3,
                results: vec![PollResult::TextAnswer]
            }
        );
        let text_answers = ctr.result_text_answers(poll_id, 0, 0);
        assert!(text_answers.0);
        assert_eq!(text_answers.1, vec![answer1, answer2, answer3])
    }

    #[test]
    fn result_text_answers() {
        let (_, mut ctr) = setup(&alice());
        let poll_id = ctr.create_poll(
            false,
            vec![question_text_answers(true)],
            2,
            100,
            String::from("Hello, world!"),
            tags(),
            String::from(""),
            String::from(""),
        );
        mk_batch_text_answers(&mut ctr, alice(), poll_id, 50);
        // depending on the lenght of the answers the limit decreases rappidly
        let text_answers = ctr._result_text_answers(poll_id, 0, 0, 30);
        assert!(!text_answers.0);
    }

    #[test]
    fn respond_iah_only_not_human() {
        let (mut ctx, mut ctr) = setup(&alice());
        let poll_id = ctr.create_poll(
            true,
            vec![question_opinion_range(false)],
            2,
            100,
            String::from("Multiple questions test!"),
            tags(),
            String::from(""),
            String::from(""),
        );
        ctx.block_timestamp = MILI_SECOND * 3;
        testing_env!(ctx);
        match ctr.on_human_verifed(
            vec![],
            true,
            alice(),
            poll_id,
            vec![Some(Answer::OpinionRange(10))],
        ) {
            Err(err) => {
                println!("Received error: {:?}", err);
                match err {
                    PollError::NoSBTs => {
                        println!("Expected error: PollError::NoSBTs")
                    }
                    _ => panic!("Unexpected error: {:?}", err),
                }
            }
            Ok(_) => panic!("Received Ok result, but expected an error"),
        }
    }

    #[test]
    fn respond_required_answer_not_provided() {
        let (mut ctx, mut ctr) = setup(&alice());
        let poll_id = ctr.create_poll(
            true,
            vec![question_opinion_range(false), question_opinion_range(true)],
            2,
            100,
            String::from("Multiple questions test!"),
            tags(),
            String::from(""),
            String::from(""),
        );
        ctx.block_timestamp = MILI_SECOND * 3;
        testing_env!(ctx);
        match ctr.on_human_verifed(
            vec![],
            false,
            alice(),
            poll_id,
            vec![Some(Answer::OpinionRange(10)), None],
        ) {
            Err(err) => {
                println!("Received error: {:?}", err);
                match err {
                    PollError::RequiredAnswer => {
                        println!("Expected error: PollError::RequiredAnswer")
                    }
                    _ => panic!("Unexpected error: {:?}", err),
                }
            }
            Ok(_) => panic!("Received Ok result, but expected an error"),
        }
    }
}
