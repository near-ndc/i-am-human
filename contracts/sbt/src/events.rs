use std::fmt;

use near_sdk::env;
use near_sdk::serde::{Deserialize, Serialize};

use crate::TokenId;
use crate::{METADATA_SPEC, SBT_STANDARD_NAME};

pub fn emit_event(event: Events) {
    // Construct the mint log as per the events standard.
    let log: EventLog = EventLog {
        standard: SBT_STANDARD_NAME.to_string(),
        version: METADATA_SPEC.to_string(),
        event,
    };
    env::log_str(&log.to_string());
}

/// Enum that represents the data type of the EventLog.
/// The enum can either be an NftMint or an NftTransfer.
#[derive(Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
#[serde(rename_all = "snake_case")]
#[serde(crate = "near_sdk::serde")]
#[non_exhaustive]
pub enum Events {
    SbtMint(Vec<SbtMintLog>),
    SbtRecover(Vec<SbtRecoverLog>),
    SbtRenew(SbtRenewLog), // TODO: use Vec?
    SbtRevoke(SbtRevokeLog),
}

/// Interface to capture data about an event
///
/// Arguments:
/// * `standard`: name of standard e.g. nep171
/// * `version`: e.g. 1.0.0
/// * `event`: associate event data
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct EventLog {
    pub standard: String,
    pub version: String,

    // `flatten` to not have "event": {<EventLogVariant>} in the JSON, just have the contents of {<EventLogVariant>}.
    #[serde(flatten)]
    pub event: Events,
}

impl fmt::Display for EventLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            "EVENT_JSON:{}",
            &serde_json::to_string(self).map_err(|_| fmt::Error)?
        ))
    }
}

/// An event emitted when a new SBT is minted.
///
/// Arguments
/// * `owner`: "account.near"
/// * `tokens`: [1, 123]
/// * `memo`: optional message
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct SbtMintLog {
    pub owner: String,
    pub tokens: Vec<TokenId>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

/// An event emitted when a recovery process succeeded to reassign SBT.
///
/// Arguments
/// * `old_owner`: "owner.near"
/// * `new_owner`: "receiver.near"
/// * `tokens`: [1, 123]
/// * `memo`: optional message
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct SbtRecoverLog {
    pub old_owner: String,
    pub new_owner: String,
    pub tokens: Vec<TokenId>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

/// An event emitted when a existing tokens are renewed.
///
/// Arguments
/// * `tokens`: [1, 123]
/// * `memo`: optional message
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct SbtRenewLog {
    pub tokens: Vec<TokenId>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

/// An event emitted when a existing tokens are revoked.
///
/// Arguments
/// * `tokens`: [1, 123]
/// * `memo`: optional message
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct SbtRevokeLog {
    pub tokens: Vec<TokenId>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    #[test]
    fn log_format_vector() {
        let expected = r#"EVENT_JSON:{"standard":"nep999","version":"1.0.0","event":"sbt_mint","data":[{"owner":"bob.near","tokens":[1,2]},{"owner":"user1.near","tokens":[4]}]}"#;
        let log = EventLog {
            standard: "nep999".to_string(),
            version: "1.0.0".to_string(),
            event: Events::SbtMint(vec![
                SbtMintLog {
                    owner: "bob.near".to_owned(),
                    tokens: vec![1, 2],
                    memo: None,
                },
                SbtMintLog {
                    owner: "user1.near".to_owned(),
                    tokens: vec![4],
                    memo: None,
                },
            ]),
        };
        assert_eq!(expected, log.to_string());
    }

    #[test]
    fn log_format_mint() {
        let expected = r#"EVENT_JSON:{"standard":"nep999","version":"1.0.0","event":"sbt_mint","data":[{"owner":"bob.near","tokens":[1,2]}]}"#;
        let log = EventLog {
            standard: "nep999".to_string(),
            version: "1.0.0".to_string(),
            event: Events::SbtMint(vec![SbtMintLog {
                owner: "bob.near".to_owned(),
                tokens: vec![1, 2],
                memo: None,
            }]),
        };
        assert_eq!(expected, log.to_string());
    }

    #[test]
    fn log_format_recovery() {
        let expected = r#"EVENT_JSON:{"standard":"nepTODO","version":"1.0.0","event":"sbt_recover","data":[{"old_owner":"user1.near","new_owner":"user2.near","tokens":[10],"memo":"process1"}]}"#;
        let log = EventLog {
            standard: SBT_STANDARD_NAME.to_string(),
            version: METADATA_SPEC.to_string(),
            event: Events::SbtRecover(vec![SbtRecoverLog {
                old_owner: "user1.near".to_string(),
                new_owner: "user2.near".to_string(),
                tokens: vec![10],
                memo: Some("process1".to_owned()),
            }]),
        };
        assert_eq!(expected, log.to_string());
    }
}
