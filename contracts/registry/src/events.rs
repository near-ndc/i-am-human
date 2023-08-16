use near_sdk::AccountId;
use sbt::{EventWrapper, NearEvent};

use crate::storage::AccountFlag;

pub fn emit_iah_flag_account(flag: crate::AccountFlag, accounts: Vec<AccountId>) {
    let event = match flag {
        AccountFlag::Blacklisted => "flag_blacklisted",
        AccountFlag::Verified => "flag_verified",
    };
    let e = NearEvent {
        standard: "iah",
        version: "1.0.0",
        event: EventWrapper {
            event,
            data: accounts, // data is a simple list of accounts to ban
        },
    };
    e.emit();
}

#[cfg(test)]
mod tests {
    use near_sdk::test_utils;

    use super::*;

    fn alice() -> AccountId {
        AccountId::new_unchecked("alice.near".to_string())
    }

    #[test]
    fn log_flag_account() {
        let accounts = vec![alice()];
        let expected1 = r#"EVENT_JSON:{"standard":"iah","version":"1.0.0","event":"flag_fake","data":["alice.near"]}"#;
        emit_iah_flag_account(AccountFlag::Blacklisted, accounts);
        assert_eq!(vec![expected1], test_utils::get_logs());
    }
}
