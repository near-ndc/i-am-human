use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{AccountId, BorshStorageKey};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};

pub type PollId = u64;

/// Helper structure for keys of the persistent collections.
#[derive(Deserialize, Serialize)]
#[serde(crate = "near_sdk::serde")]
pub enum PollQuestionAnswer {
    YesNo(bool),
    TextChoices(Vec<String>), // should respect the min_choices, max_choices
    PictureChoices(Vec<String>), // should respect the min_choices, max_choices
    OpinionScale(usize), // should be a number between 0 and 10
    TextAnswer(String),
}

/// Helper structure for keys of the persistent collections.
#[derive(Deserialize, Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct PollQuestion{
    question_type: PollQuestionAnswer, // required
    required: bool, // required, if true users can't vote without having an answer for this question
    title: String, // required
    description: Option<String>, // optional
    image: Option<String>, // optional
    labels: Option<(String, String, String)>, // if applicable, labels for the opinion scale question
    choices: Option<Vec<usize>>, // if applicable, choices for the text and picture choices question
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Poll{
    verified_humans_only: bool, // required, if true only verified humans can vote, if false anyone can vote
    questions: Vec<PollQuestion>, // required, a poll can have any number of questions
    starts_at: usize, // required, time in milliseconds
    end_at: usize, // required, time in milliseconds
      title: String, // required
    tags: Vec<String>, // can be an empty vector
    description: Option<String>, // optional
    link: Option<String>, // optional
      created_at: usize, // should be assigned by the smart contract not the user, time in milliseconds
}
  
#[derive(Deserialize, Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Vote {
    answers: Vec<(usize, PollQuestionAnswer)>, // question_id, answer
      created_at: usize, // should be assigned by the smart contract not the user, time in milliseconds
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
  pub struct PollResult {
    status: Status, 
    results: Vec<(usize, Vec<PollQuestionAnswer>)>, // question_id, list of answers
    number_of_participants: u64,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub enum Status {
    NotStarted,
    Active,
    Finished
}

#[derive(BorshSerialize, BorshStorageKey)]
pub enum StorageKey {
    Polls,
    Another,
}