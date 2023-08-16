use near_sdk::AccountId;
use sbt::{EventPayload, NearEvent};

use crate::storage::AccountFlag;

pub fn emit_iah_flag_account(flag: crate::AccountFlag, accounts: Vec<AccountId>) {
    let event = match flag {
        AccountFlag::Blacklisted => "flag_blacklisted",
        AccountFlag::Verified => "flag_verified",
    };
    NearEvent {
        standard: "i_am_human",
        version: "1.0.0",
        event: EventPayload {
            event,
            data: accounts, // data is a simple list of accounts to ban
        },
    }
    .emit();
}

#[cfg(test)]
mod tests {
    use near_sdk::test_utils;

    use super::*;

    fn acc(idx: u8) -> AccountId {
        AccountId::new_unchecked(format!("user-{}.near", idx))
    }

    #[test]
    fn log_flag_account() {
        let expected1 = r#"EVENT_JSON:{"standard":"i_am_human","version":"1.0.0","event":"flag_blacklisted","data":["user-1.near"]}"#;
        emit_iah_flag_account(AccountFlag::Blacklisted, vec![acc(1)]);
        assert_eq!(vec![expected1], test_utils::get_logs());

        let expected2 = r#"EVENT_JSON:{"standard":"i_am_human","version":"1.0.0","event":"flag_verified","data":["user-4.near","user-2.near"]}"#;
        emit_iah_flag_account(AccountFlag::Verified, vec![acc(4), acc(2)]);
        assert_eq!(vec![expected1, expected2], test_utils::get_logs());
    }
}
