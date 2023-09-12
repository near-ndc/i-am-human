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
pub const MAX_TEXT_ANSWER_LEN: usize = 500; // TODO: decide on the maximum length of the text answers to

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// map of all polls
    pub polls: UnorderedMap<PollId, Poll>,
    /// map of all results summarized
    pub results: LookupMap<PollId, Results>,
    /// map of all answers, (poll, user) -> vec of answers
    pub answers: LookupMap<(PollId, AccountId), Vec<Option<Answer>>>,
    /// text answers are stored in a separate map. Key is a (pollId, question index).
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
            answers: LookupMap::new(StorageKey::Answers),
            text_answers: LookupMap::new(StorageKey::TextAnswers),
            registry,
            next_poll_id: 1,
        }
    }

    /**********
     * QUERIES
     **********/

    /// Returns the poll details. If poll not found returns None.
    pub fn poll(&self, poll_id: PollId) -> Option<Poll> {
        self.polls.get(&poll_id)
    }

    /// Returns caller response to the specified poll. It doesn't return text responses of the given poll ID.
    pub fn my_response(&self, poll_id: PollId) -> Option<Vec<Option<Answer>>> {
        let caller = env::predecessor_account_id();
        self.answers.get(&(poll_id, caller))
    }

    /// Returns poll results (except for text answers), if poll not found returns None.
    pub fn results(&self, poll_id: u64) -> Option<Results> {
        self.results.get(&poll_id)
    }

    /// Returns text answers in rounds. Starts from the question id provided. Needs to be called until true is returned.
    pub fn text_answers(
        &self,
        poll_id: u64,
        question: usize,
        from_answer: usize,
    ) -> TextResponse<(Vec<String>, bool)> {
        // We cannot return more than 100 due to gas limit per txn.
        self._text_answers(poll_id, question, from_answer, 100)
    }

    /// Returns a fixed value of answers
    // Function must be called until true is returned -> meaning all the answers were returned
    // `question` must be an index of the text question in the poll
    pub fn _text_answers(
        &self,
        poll_id: u64,
        question: usize,
        from_answer: usize,
        limit: usize,
    ) -> TextResponse<(Vec<String>, bool)> {
        let poll = match self.polls.get(&poll_id) {
            Some(poll) => poll,
            None => return TextResponse::PollNotFound,
        };

        match poll.questions.get(question) {
            Some(questions) => questions,
            None => return TextResponse::QuestionNotFound,
        };

        let text_answers = match self.text_answers.get(&(poll_id, question)) {
            Some(text_answers) => text_answers,
            None => return TextResponse::QuestionWrongType,
        };
        let to_return;
        let mut finished = false;
        if from_answer + limit > text_answers.len() as usize {
            to_return = text_answers.to_vec()[from_answer..].to_vec();
            finished = true;
        } else {
            to_return = text_answers.to_vec()[from_answer..from_answer + limit].to_vec();
        }
        TextResponse::Ok((to_return, finished))
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
            "poll start must be in the future".to_string()
        );
        let poll_id = self.next_poll_id;
        self.next_poll_id += 1;
        self.initialize_results(poll_id, &questions);
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

    /// Allows user to respond to a poll, once the answers are submited they cannot be changed.
    /// it panics if
    /// - poll not found
    /// - poll not active
    /// - user alredy answered
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

        self.assert_active(poll_id)?;

        self.assert_answered(poll_id, &caller)?;
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
            self.on_human_verifed(vec![], false, caller, poll_id, answers)?
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
        // Check for IAH requirement if iah_only is set
        if iah_only && tokens.is_empty() {
            return Err(PollError::NotIAH);
        }

        // Retrieve questions and poll results
        let questions: Vec<Question> = self.polls.get(&poll_id).expect("poll not found").questions;
        let mut poll_results = self.results.get(&poll_id).expect("results not found");

        // Check if the number of answers matches the number of questions
        if questions.len() != answers.len() {
            return Err(PollError::IncorrectAnswerVector);
        }

        // Initialize unwrapped_answers vector
        let mut unwrapped_answers: Vec<Option<Answer>> = Vec::new();

        for i in 0..questions.len() {
            let q = &questions[i];
            let a = &answers[i];
            if q.required && a.is_none() {
                return Err(PollError::RequiredAnswer(i));
            }

            match (a, &mut poll_results.results[i]) {
                (Some(Answer::YesNo(response)), PollResult::YesNo((yes_count, no_count))) => {
                    if *response {
                        *yes_count += 1;
                    } else {
                        *no_count += 1;
                    }
                }
                (Some(Answer::TextChoices(choices)), PollResult::TextChoices(results))
                | (Some(Answer::PictureChoices(choices)), PollResult::PictureChoices(results)) => {
                    for (j, choice) in choices.iter().enumerate() {
                        if *choice {
                            results[j] += 1;
                        }
                    }
                }
                (Some(Answer::OpinionRange(opinion)), PollResult::OpinionRange(results)) => {
                    if *opinion < 1 || *opinion > 10 {
                        return Err(PollError::OpinionRange);
                    }
                    results.sum += *opinion as u64;
                    results.num += 1;
                }
                (Some(Answer::TextAnswer(answer)), PollResult::TextAnswer) => {
                    let mut answers = self
                        .text_answers
                        .get(&(poll_id, i))
                        .expect(&format!("question not found for index {:?}", i));

                    if answer.len() > MAX_TEXT_ANSWER_LEN {
                        return Err(PollError::AnswerTooLong(answer.len()));
                    }
                    answers.push(answer);
                    self.text_answers.insert(&(poll_id, i), &answers);
                }
                // if the answer is not provided for a question None is pushed as an anser to keep the integrity
                (None, _) => {
                    unwrapped_answers.push(None);
                }
                (_, _) => return Err(PollError::WrongAnswer),
            }
            if answers[i].is_some() {
                // None case is handled in the `match` statement.
                unwrapped_answers.push(Some(answers[i].clone().unwrap()));
            }
        }
        // Update answers for the caller
        let mut caller_answers = self
            .answers
            .get(&(poll_id, caller.clone()))
            .unwrap_or(Vec::new());
        caller_answers.append(&mut unwrapped_answers);
        self.answers
            .insert(&(poll_id, caller.clone()), &caller_answers);

        // Update participants count and poll results
        poll_results.participants += 1;

        // Update results and emit response event
        self.results.insert(&poll_id, &poll_results);
        emit_respond(poll_id, caller);

        Ok(())
    }

    /**********
     * INTERNAL
     **********/

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

    fn assert_answered(&self, poll_id: PollId, caller: &AccountId) -> Result<(), PollError> {
        if self.answers.get(&(poll_id, caller.clone())).is_some() {
            return Err(PollError::AlredyAnswered);
        }
        Ok(())
    }

    fn initialize_results(&mut self, poll_id: PollId, questions: &[Question]) {
        let mut index = 0;
        let results: Vec<PollResult> = questions
            .iter()
            .map(|question| {
                let result = match &question.question_type {
                    Answer::YesNo(_) => PollResult::YesNo((0, 0)),
                    Answer::TextChoices(choices) => PollResult::TextChoices(vec![0; choices.len()]),
                    Answer::PictureChoices(_) => PollResult::PictureChoices(Vec::new()),
                    Answer::OpinionRange(_) => {
                        PollResult::OpinionRange(OpinionRangeResult { sum: 0, num: 0 })
                    }
                    Answer::TextAnswer(_) => {
                        self.text_answers
                            .insert(&(poll_id, index), &Vector::new(StorageKey::TextAnswers));
                        PollResult::TextAnswer
                    }
                };
                index += 1;
                result
            })
            .collect();

        self.results.insert(
            &poll_id,
            &Results {
                status: Status::NotStarted,
                participants: 0,
                results,
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
        Status, TextResponse, RESPOND_COST,
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
        ctx: &mut VMContext,
        ctr: &mut Contract,
        predecessor: AccountId,
        poll_id: PollId,
        num_answers: u64,
    ) {
        for _ in 0..num_answers {
            testing_env!(ctx.clone());
            let res = ctr.on_human_verifed(
                vec![],
                false,
                predecessor.clone(),
                poll_id,
                vec![Some(Answer::TextAnswer(
                    "wRjLbQZKutS0PCDx7F9pm5HgdO2h6vYcnlzBq3sEkU1f84aMyViAXTNjIoWPeLrVGvMm8
                    HQZ7ij4J9gKdmMIsN5FB2wXfYuEkRlLTbn3DpGePo1VSqaAhYcC6W0Ou8ztvrxXnaxVbX1
                    lMoXJ1YKvIksRnmQHD0VdW9GZrATg28pzUhqyfcBCjaoR6xs45Lu73Fw1PtevOYINaan3
                    wRjLbQZKutS0PCDx7F9pm5HgdO2h6vYcnlzBq3sEkU1f84aMyViAXTNjIoWPeLrVGvMm8
                    HQZ7ij4J9gKdmMIsN5FB2wXfYuEkRlLTbn3DpGePo1VSqaAhYcC6W0Ou8ztvrxXnaxVbX1
                    lMoXJ1YKvIksRnmQHD0VdW9GZrATg28pzUhqyfcBCjao"
                        .to_owned(),
                ))],
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
        let expected_event = r#"EVENT_JSON:{"standard":"ndc-easy-poll","version":"1.0.0","event":"create_poll","data":{"poll_id":1}}"#;
        assert!(test_utils::get_logs().len() == 1);
        assert_eq!(test_utils::get_logs()[0], expected_event);
    }

    #[test]
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
        assert!(ctr.my_response(poll_id).is_none());
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
        assert_eq!(res.unwrap(), vec![None, Some(Answer::YesNo(true))])
    }

    #[test]
    fn results_poll_not_found() {
        let (_, ctr) = setup(&alice());
        assert!(ctr.results(1).is_none());
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
        assert_eq!(res.unwrap(), expected);
    }

    #[test]
    fn result_text_answers_poll_not_found() {
        let (_, ctr) = setup(&alice());
        match ctr.text_answers(0, 0, 0) {
            TextResponse::PollNotFound => (),
            other => panic!("Expected TextResponse::PollNotFound, but got {:?}", other),
        };
    }

    #[test]
    fn result_text_answers_question_not_found() {
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
        match ctr.text_answers(poll_id, 1, 0) {
            TextResponse::QuestionNotFound => (),
            other => panic!(
                "Expected TextResponse::QuestionNotFound, but got {:?}",
                other
            ),
        };
    }

    #[test]
    fn result_text_answers_question_wrong_type() {
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
        match ctr.text_answers(poll_id, 0, 0) {
            TextResponse::QuestionWrongType => (),
            other => panic!(
                "Expected TextResponse::QuestionWrongType, but got {:?}",
                other
            ),
        };
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

        let expected_event = r#"EVENT_JSON:{"standard":"ndc-easy-poll","version":"1.0.0","event":"respond","data":{"poll_id":1,"responder":"alice.near"}}"#;
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

        let results = ctr.results(poll_id);
        assert_eq!(
            results.unwrap(),
            Results {
                status: Status::NotStarted,
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
            results.unwrap(),
            Results {
                status: Status::NotStarted,
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
            results.unwrap(),
            Results {
                status: Status::NotStarted,
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
            results.unwrap(),
            Results {
                status: Status::NotStarted,
                participants: 3,
                results: vec![PollResult::TextAnswer]
            }
        );
        let text_answers = ctr.text_answers(poll_id, 0, 0);
        assert_eq!(
            text_answers,
            TextResponse::Ok((vec![answer1, answer2, answer3], true))
        );
    }

    #[test]
    fn result_text_answers() {
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
        mk_batch_text_answers(&mut ctx, &mut ctr, alice(), poll_id, 200);
        // depending on the lenght of the answers the limit decreases rappidly
        let text_answers = ctr._text_answers(poll_id, 0, 0, 100);
        match text_answers {
            TextResponse::Ok((_, false)) => {}
            _ => panic!(
                "Expected TextResponse::Ok with false, but got {:?}",
                text_answers
            ),
        }
        let text_answers = ctr._text_answers(poll_id, 0, 101, 100);
        match text_answers {
            TextResponse::Ok((_, true)) => {}
            _ => panic!(
                "Expected TextResponse::Ok with true, but got {:?}",
                text_answers
            ),
        }
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
                    PollError::NotIAH => {
                        println!("Expected error: PollError::NotIAH")
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
                    PollError::RequiredAnswer(1) => {
                        println!("Expected error: PollError::RequiredAnswer")
                    }
                    _ => panic!("Unexpected error: {:?}", err),
                }
            }
            Ok(_) => panic!("Received Ok result, but expected an error"),
        }
    }
}
