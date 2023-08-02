use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{AccountId, BorshStorageKey};

pub type PollId = u64;

/// Helper structure for keys of the persistent collections.
#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub enum Answer {
    YesNo(bool),
    TextChoices(Vec<usize>),    // should respect the min_choices, max_choices
    PictureChoices(Vec<usize>), // should respect the min_choices, max_choices
    OpinionScale(u64),          // should be a number between 0 and 10
    TextAnswer(String),
}
#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(PartialEq, Debug))]
#[serde(crate = "near_sdk::serde")]
pub enum PollResult {
    YesNo((u32, u32)),                // yes, no
    TextChoices(Vec<u32>),            // should respect the min_choices, max_choices
    PictureChoices(Vec<u32>),         // should respect the min_choices, max_choices
    OpinionScale(OpinionScaleResult), // mean value
    TextAnswer(Vec<String>),
}
#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(PartialEq, Debug))]
#[serde(crate = "near_sdk::serde")]
pub struct OpinionScaleResult {
    pub sum: u32,
    pub num: u32,
}

/// Helper structure for keys of the persistent collections.
#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Question {
    pub question_type: Answer,                    // required
    pub required: bool, // required, if true users can't vote without having an answer for this question
    pub title: String,  // required
    pub description: Option<String>, // optional
    pub image: Option<String>, // optional
    pub labels: Option<(String, String, String)>, // if applicable, labels for the opinion scale question
    pub choices: Option<Vec<String>>, // if applicable, choices for the text and picture choices question TODO: make sure we dont need it
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Poll {
    pub iah_only: bool, // required, if true only verified humans can vote, if false anyone can vote
    pub questions: Vec<Question>, // required, a poll can have any number of questions
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
    answers: Vec<(usize, Answer)>, // question_id, answer
    created_at: usize, // should be assigned by the smart contract not the user, time in milliseconds
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(PartialEq, Debug))]
#[serde(crate = "near_sdk::serde")]
pub struct Results {
    pub status: Status,
    pub number_of_participants: u64,
    pub results: Vec<PollResult>, // question_id, result (sum of yes etc.)
}

#[derive(Serialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct Answers {
    status: Status,
    number_of_participants: u64,
    answers: Vec<(usize, Vec<Answer>)>, // question_id, list of answers
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(PartialEq, Debug))]
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
