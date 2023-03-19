use std::fmt;

use near_sdk::env::log_str;
use near_sdk::serde::{Deserialize, Serialize};

pub const VERSION: &str = "1.0.0";

pub(crate) fn emit_event(event: EventKind) {
    // Construct the mint log as per the events standard.
    let e = Event {
        // standard: LOG_NAME.to_string(),
        version: VERSION.to_string(),
        event,
    };
    log_str(&e.to_string());
}

/// Interface to capture data about an event
///
/// Arguments:
/// * `standard`: name of standard e.g. nep171
/// * `version`: e.g. 1.0.0
/// * `event`: associate event data
#[derive(Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq))]
#[serde(crate = "near_sdk::serde")]
pub struct Event {
    // TODO: `standard` is specified by NEP, but nor the indexer nor the near-contract-standards
    // provide that field
    // pub standard: String,
    pub version: String,

    // `flatten` to not have "event": {<EventLogVariant>} in the JSON, just have the contents of {<EventLogVariant>}.
    #[serde(flatten)]
    pub event: EventKind,
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            "EVENT_JSON:{}",
            &serde_json::to_string(self).map_err(|_| fmt::Error)?
        ))
    }
}

/// Enum that represents the data type of the EventLog.
#[derive(Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq))]
#[serde(tag = "event", content = "data")]
#[serde(rename_all = "snake_case")]
#[serde(crate = "near_sdk::serde")]
#[non_exhaustive]
pub enum EventKind {
    Blacklist(Vec<Blacklist>),
}

/// An event emitted when a human protocol SBT blacklists an account.
///
/// Arguments
/// * `caller`: "sbt-poap.near"
/// * `account`: "bob.near"
/// * `memo`: optional message
#[derive(Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq))]
#[serde(crate = "near_sdk::serde")]
pub struct Blacklist {
    pub caller: String,
    pub account: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

impl Blacklist {
    pub fn emit(self) {
        emit_event(EventKind::Blacklist(vec![self]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use near_contract_standards::non_fungible_token::events::NftMint;

    #[test]
    fn log_format_blacklist() {
        let expected = r#"EVENT_JSON:{"version":"1.0.0","event":"blacklist","data":[{"caller":"bob.near","account":"charlie.near"},{"caller":"bob2.near","account":"charlie2.near","memo":"my memo"}]}"#;
        let log = Event {
            version: VERSION.to_string(),
            event: EventKind::Blacklist(vec![
                Blacklist {
                    caller: "bob.near".to_owned(),
                    account: "charlie.near".to_owned(),
                    memo: None,
                },
                Blacklist {
                    caller: "bob2.near".to_owned(),
                    account: "charlie2.near".to_owned(),
                    memo: Some("my memo".to_owned()),
                },
            ]),
        };
        assert_eq!(expected, log.to_string());
    }
}
