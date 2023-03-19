use near_sdk::env::log_str;
use near_sdk::serde::Serialize;
use near_sdk::{env, AccountId};

use sbt::NearEvent;

pub const VERSION: &str = "1.0.0";
/// name of the standard
pub const STANDARD_NAME: &str = "nepXXX";

/// Enum that represents the data type of the EventLog.
#[derive(Serialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq))]
#[serde(tag = "event", content = "data")]
#[serde(rename_all = "snake_case")]
#[serde(crate = "near_sdk::serde")]
#[non_exhaustive]
pub enum EventKind<'a> {
    Blacklist(Vec<Blacklist<'a>>),
}

impl EventKind<'_> {
    /// creates a string compatible with NEAR event standard
    pub fn to_json_event_string(self) -> String {
        let e = NearEvent {
            standard: STANDARD_NAME,
            version: VERSION,
            event: self,
        };
        let s = serde_json::to_string(&e)
            .ok()
            .unwrap_or_else(|| env::abort());
        format!("EVENT_JSON:{}", s)
    }

    pub fn emit(self) {
        log_str(&self.to_json_event_string());
    }
}

/// An event emitted when a human protocol SBT blacklists an account.
///
/// Arguments
/// * `caller`: "sbt-poap.near"
/// * `account`: "bob.near"
/// * `memo`: optional message
#[derive(Serialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq))]
#[serde(crate = "near_sdk::serde")]
pub struct Blacklist<'a> {
    pub caller: &'a AccountId,
    pub account: &'a AccountId,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

impl Blacklist<'_> {
    pub fn emit(self) {
        EventKind::Blacklist(vec![self]).emit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> AccountId {
        AccountId::new_unchecked("alice.near".to_string())
    }

    fn bob() -> AccountId {
        AccountId::new_unchecked("bob.near".to_string())
    }

    fn charlie() -> AccountId {
        AccountId::new_unchecked("charlie.near".to_string())
    }

    #[test]
    fn log_format_blacklist() {
        let alice = alice();
        let bob = bob();
        let charlie = charlie();
        let expected = r#"EVENT_JSON:{"standard":"nepXXX","version":"1.0.0","event":"blacklist","data":[{"caller":"bob.near","account":"charlie.near"},{"caller":"charlie.near","account":"alice.near","memo":"my memo"}]}"#;
        let event = EventKind::Blacklist(vec![
            Blacklist {
                caller: &bob,
                account: &charlie,
                memo: None,
            },
            Blacklist {
                caller: &charlie,
                account: &alice,
                memo: Some("my memo".to_owned()),
            },
        ]);
        assert_eq!(expected, event.to_json_event_string());
    }
}
