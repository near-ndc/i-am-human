use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::BorshStorageKey;

pub type PollId = u64;

/// Helper structure for keys of the persistent collections.
#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(PartialEq, Debug))]
#[serde(crate = "near_sdk::serde")]
pub enum Answer {
    YesNo(bool),
    TextChoices(Vec<bool>),    // should respect the min_choices, max_choices
    PictureChoices(Vec<bool>), // should respect the min_choices, max_choices
    OpinionRange(u8),          // should be a number between 0 and 10
    TextAnswer(String),
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(PartialEq, Debug))]
#[serde(crate = "near_sdk::serde")]
pub enum PollResult {
    YesNo((u32, u32)),                // yes, no
    TextChoices(Vec<u32>),            // should respect the min_choices, max_choices
    PictureChoices(Vec<u32>),         // should respect the min_choices, max_choices
    OpinionRange(OpinionRangeResult), // mean value
    TextAnswer, // indicates whether the question exist or not, the answers are stored in a different struct called `TextAnswers`
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(PartialEq, Debug))]
#[serde(crate = "near_sdk::serde")]
pub struct OpinionRangeResult {
    pub sum: u64,
    pub num: u64,
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
    pub max_choices: Option<u32>,
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
    pub description: String, // can be an empty string
    pub link: String,   // can be an empty string
    pub created_at: u64, // time in milliseconds, should be assigned by the smart contract not a user.
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(PartialEq, Debug))]
#[serde(crate = "near_sdk::serde")]
pub struct Results {
    pub status: Status,
    pub participants_num: u64,    // number of participants
    pub results: Vec<PollResult>, // question_id, result (sum of yes etc.)
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
    Participants,
}
