use near_sdk::{serde::Serialize, AccountId};
use sbt::{EventPayload, NearEvent};

use crate::storage::AccountFlag;

fn emit_iah_event<T: Serialize>(event: EventPayload<T>) {
    NearEvent {
        standard: "i_am_human",
        version: "1.0.0",
        event,
    }
    .emit();
}

pub(crate) fn emit_iah_flag_accounts(flag: crate::AccountFlag, accounts: Vec<AccountId>) {
    let event = match flag {
        AccountFlag::Blacklisted => "flag_blacklisted",
        AccountFlag::Verified => "flag_verified",
        AccountFlag::GovBan => "flag_govban",
    };
    emit_iah_event(EventPayload {
        event,
        data: accounts, // data is a simple list of accounts to flag (Verify or Blacklist)
    });
}

pub(crate) fn emit_iah_unflag_accounts(accounts: Vec<AccountId>) {
    emit_iah_event(EventPayload {
        event: "unflag",
        data: accounts, // data is a simple list of accounts to unflag
    });
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
        emit_iah_flag_accounts(AccountFlag::Blacklisted, vec![acc(1)]);
        assert_eq!(vec![expected1], test_utils::get_logs());

        let expected2 = r#"EVENT_JSON:{"standard":"i_am_human","version":"1.0.0","event":"flag_verified","data":["user-4.near","user-2.near"]}"#;
        emit_iah_flag_accounts(AccountFlag::Verified, vec![acc(4), acc(2)]);
        assert_eq!(vec![expected1, expected2], test_utils::get_logs());

        let expected3 = r#"EVENT_JSON:{"standard":"i_am_human","version":"1.0.0","event":"unflag","data":["user-4.near","user-3.near"]}"#;
        emit_iah_unflag_accounts(vec![acc(4), acc(3)]);
        assert_eq!(
            vec![expected1, expected2, expected3],
            test_utils::get_logs()
        );
    }
}
