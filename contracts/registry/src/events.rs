use near_sdk::serde::Serialize;
use near_sdk::{env, AccountId};
use sbt::{EventWrapper, NearEvent};

use crate::storage::AccountFlag;

pub fn emit_iah_account_flag(flag: crate::AccountFlag, account: AccountId) {
    let event = match flag {
        AccountFlag::Black => "flag_fake",
        AccountFlag::White => "flag_trusted",
    };
    let e = NearEvent {
        standard: "iah",
        version: "1.0.0",
        event: EventWrapper {
            event,
            data: vec![account], // data is a simple list of accounts to ban
        },
    };
    e.emit();
}
