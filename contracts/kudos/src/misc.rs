use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};

/// This type represents this contract state
///
/// Public methods are available only while this contract is in [`Running`](RunningState::Running) state
#[derive(BorshDeserialize, BorshSerialize, PartialEq)]
pub enum RunningState {
    Running,
    Paused,
}
