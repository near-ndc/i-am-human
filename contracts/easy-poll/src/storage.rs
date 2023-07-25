use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{AccountId, BorshStorageKey};

pub type PollId = u64;

/// Helper structure for keys of the persistent collections.
#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize)]
#[serde(crate = "near_sdk::serde")]
pub enum PollQuestionAnswer {
    YesNo(bool),
    TextChoices(Vec<usize>),    // should respect the min_choices, max_choices
    PictureChoices(Vec<usize>), // should respect the min_choices, max_choices
    OpinionScale(u64),          // should be a number between 0 and 10
    TextAnswer(String),
}

pub enum PollQuestionResult {
    YesNo((u32, u32)),
    TextChoices(Vec<u32>),    // should respect the min_choices, max_choices
    PictureChoices(Vec<u32>), // should respect the min_choices, max_choices
    OpinionScale(OpinionScaleResult), // mean value
}

pub struct OpinionScaleResult {
    pub sum: u32,
    pub num: u32,
}

/// Helper structure for keys of the persistent collections.
#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct PollQuestion {
    pub question_type: PollQuestionAnswer,        // required
    pub required: bool, // required, if true users can't vote without having an answer for this question
    pub title: String,  // required
    pub description: Option<String>, // optional
    pub image: Option<String>, // optional
    pub labels: Option<(String, String, String)>, // if applicable, labels for the opinion scale question
    pub choices: Option<Vec<usize>>, // if applicable, choices for the text and picture choices question
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Poll {
    pub iah_only: bool, // required, if true only verified humans can vote, if false anyone can vote
    pub questions: Vec<PollQuestion>, // required, a poll can have any number of questions
    pub starts_at: u64, // required, time in milliseconds
    pub ends_at: u64,   // required, time in milliseconds
    pub title: String,  // required
    pub tags: Vec<String>, // can be an empty vector
    pub description: Option<String>, // optional
    pub link: Option<String>, // optional
    pub created_at: u64, // should be assigned by the smart contract not the user, time in milliseconds
}

#[derive(Deserialize, Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct PollResponse {
    answers: Vec<(usize, PollQuestionAnswer)>, // question_id, answer
    created_at: usize, // should be assigned by the smart contract not the user, time in milliseconds
}

#[derive(Deserialize, Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct PollResults {
    pub status: Status,
    pub number_of_participants: u64,
    pub answers: Vec<(usize, PollQuestionResult)>, // question_id, answer
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
    Finished,
}

#[derive(BorshSerialize, BorshStorageKey)]
pub enum StorageKey {
    Polls,
    Results,
    Answers,
}
