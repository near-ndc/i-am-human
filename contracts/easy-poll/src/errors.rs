use near_sdk::env::panic_str;
use near_sdk::FunctionError;

use crate::Poll;

/// Contract errors
#[cfg_attr(not(target_arch = "wasm32"), derive(PartialEq))]
#[derive(Debug)]
pub enum PollError {
    RequiredAnswer,
    NoSBTs,
    NotFound,
    NotActive,
    OpinionRange,
}

impl FunctionError for PollError {
    fn panic(&self) -> ! {
        match self {
            PollError::RequiredAnswer => {
                panic_str("Answer to a required question was not provided")
            }
            PollError::NoSBTs => panic_str("voter is not a verified human"),
            PollError::NotFound => panic_str("poll not found"),
            PollError::NotActive => panic_str("poll is not active"),
            PollError::OpinionScale => panic_str("opinion must be between 0 and 10"),
        }
    }
}
