use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{UnorderedMap, UnorderedSet};
use near_sdk::{env, near_bindgen, AccountId, PanicOnDefault};

use crate::events::*;
mod events;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    // registry of blacklisted accounts by issuer
    pub blacklist: UnorderedMap<AccountId, UnorderedSet<AccountId>>,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    #[init]
    pub fn new() -> Self {
        Self {
            blacklist: UnorderedMap::new(b'b'),
        }
    }

    pub fn blacklist(&mut self, account: AccountId, memo: Option<String>) {
        // TODO: add storage fees

        let caller = env::predecessor_account_id();
        let mut s = self
            .blacklist
            .get(&caller)
            // TODO: check if we can use this prefix, or if we need to use a
            // globally unique one
            .unwrap_or_else(|| UnorderedSet::new(caller.as_bytes()));
        s.insert(&account);
        self.blacklist.insert(&caller, &s);
        let event = BlacklistLog {
            caller,
            account,
            memo,
        };
        log(event);
    }

    /// checks if an `account` was blacklisted by `ctr` contract in an event
    /// of SBT (soulbound token) recovery process.
    pub fn is_blacklisted(&self, ctr: AccountId, account: AccountId) -> bool {
        self.blacklist
            .get(&ctr)
            .map_or(false, |s| s.contains(&account))
    }
}

fn log(event: BlacklistLog) {
    // Construct the mint log as per the events standard.
    let e = EventLog {
        standard: LOG_NAME.to_string(),
        version: VERSION.to_string(),
        event,
    };
    env::log_str(&e.to_string());
}
