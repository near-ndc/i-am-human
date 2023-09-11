use near_sdk::env::panic_str;
use near_sdk::FunctionError;

use crate::MAX_TEXT_ANSWER_LEN;

/// Contract errors
#[cfg_attr(not(target_arch = "wasm32"), derive(PartialEq))]
#[derive(Debug)]
pub enum PollError {
    RequiredAnswer(usize),
    NoSBTs,
    NotFound,
    NotActive,
    OpinionRange,
    WrongAnswer,
    IncorrectAnswerVector,
    AlredyAnswered,
    AnswerTooLong(usize),
}

impl FunctionError for PollError {
    fn panic(&self) -> ! {
        match self {
            PollError::RequiredAnswer(index) => {
                panic_str(&format!("Answer to a required question index={} was not provided",index))
            }
            PollError::NoSBTs => panic_str("voter is not a verified human"),
            PollError::NotFound => panic_str("poll not found"),
            PollError::NotActive => panic_str("poll is not active"),
            PollError::OpinionRange => panic_str("opinion must be between 0 and 10"),
            PollError::WrongAnswer => {
                panic_str("answer provied does not match the expected question")
            },
            PollError::IncorrectAnswerVector => panic_str("the answer vector provided is incorrect and does not match the questions in the poll"),
            PollError::AlredyAnswered => panic_str("user has already answered"),
            PollError::AnswerTooLong(len) => {panic_str(&format!("the answer too long, max_len:{}, got:{}", MAX_TEXT_ANSWER_LEN, len))}
        }
    }
}
