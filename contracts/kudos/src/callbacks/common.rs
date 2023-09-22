use crate::{Contract, ContractExt};
use near_sdk::{env, near_bindgen};

#[near_bindgen]
impl Contract {
    #[private]
    pub fn on_failure(&mut self, error: String) {
        env::panic_str(&error)
    }
}
