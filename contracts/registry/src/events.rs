use std::fmt;

use near_sdk::env::log_str;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::AccountId;

pub const LOG_NAME: &str = "sbt-class"; // TODO -- need to define a proper name
pub const VERSION: &str = "1.0.0";

/// Interface to capture data about an event
///
/// Arguments:
/// * `standard`: name of standard e.g. nep171
/// * `version`: e.g. 1.0.0
/// * `event`: associate event data
#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct EventLog {
    pub standard: String,
    pub version: String,

    // `flatten` to not have "event": {<EventLogVariant>} in the JSON, just have the contents of {<EventLogVariant>}.
    #[serde(flatten)]
    pub event: BlacklistLog,
}

impl fmt::Display for EventLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            "EVENT_JSON:{}",
            &serde_json::to_string(self).map_err(|_| fmt::Error)?
        ))
    }
}

/// An event emitted when a human protocol SBT blacklists an account.
///
/// Arguments
/// * `caller`: "sbt-poap.near"
/// * `account`: "bob.near"
/// * `memo`: optional message
#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct BlacklistLog {
    pub caller: AccountId,
    pub account: AccountId,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

pub(crate) fn emit_event(event: BlacklistLog) {
    // Construct the mint log as per the events standard.
    let e = EventLog {
        standard: LOG_NAME.to_string(),
        version: VERSION.to_string(),
        event,
    };
    log_str(&e.to_string());
}
