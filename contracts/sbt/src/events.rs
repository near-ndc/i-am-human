use near_sdk::env;
use near_sdk::serde::{Deserialize, Serialize};

use crate::METADATA_SPEC;
use crate::{TokenId, STANDARD_NAME};

pub fn emit_event(event: Nep393EventKind) {
    env::log_str(&Event::from(event).to_string());
}

/// Enum that represents the data type of the EventLog.
#[derive(Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Clone))]
#[serde(tag = "event", content = "data")]
#[serde(rename_all = "snake_case")]
#[serde(crate = "near_sdk::serde")]
#[non_exhaustive]
pub enum Nep393EventKind {
    SbtMint(Vec<SbtMint>),
    SbtRecover(Vec<SbtRecover>),
    // no need to use vector of SbtRenew and SbtRevoke events, because the event already has
    // list of token_ids
    SbtRenew(SbtRenew),
    SbtRevoke(SbtRevoke),
}

impl Nep393EventKind {
    /// creates a string compatible with NEAR event standard
    pub fn to_json_event_string(self) -> String {
        format!("EVENT_JSON:{}", Event::from(self).to_string())
    }
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
#[serde(rename_all = "snake_case")]
struct Event {
    pub standard: String,
    pub version: String,

    // `flatten` to not have "event": {<EventLogVariant>} in the JSON, just have the contents of {<EventLogVariant>}.
    #[serde(flatten)]
    pub event: Nep393EventKind,
}

impl Event {
    fn to_string(&self) -> String {
        serde_json::to_string(self)
            .ok()
            .unwrap_or_else(|| env::abort())
    }
}

impl From<Nep393EventKind> for Event {
    fn from(event: Nep393EventKind) -> Self {
        // Construct the mint log as per the events standard.
        Self {
            standard: STANDARD_NAME.to_string(),
            version: METADATA_SPEC.to_string(),
            event,
        }
    }
}

/// An event emitted when a new SBT is minted.
///
/// Arguments
/// * `owner`: "account.near"
/// * `tokens`: [1, 123]
/// * `memo`: optional message
#[derive(Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Clone))]
#[serde(crate = "near_sdk::serde")]
pub struct SbtMint {
    pub owner: String,
    pub tokens: Vec<TokenId>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

impl SbtMint {
    pub fn emit(self) {
        emit_event(Nep393EventKind::SbtMint(vec![self]));
    }
}

/// An event emitted when a recovery process succeeded to reassign SBT.
///
/// Arguments
/// * `old_owner`: "owner.near"
/// * `new_owner`: "receiver.near"
/// * `tokens`: [1, 123]
/// * `memo`: optional message
#[derive(Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Clone))]
#[serde(crate = "near_sdk::serde")]
pub struct SbtRecover {
    pub old_owner: String,
    pub new_owner: String,
    pub tokens: Vec<TokenId>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

impl SbtRecover {
    pub fn emit(self) {
        emit_event(Nep393EventKind::SbtRecover(vec![self]));
    }
}

/// An event emitted when a existing tokens are renewed.
///
/// Arguments
/// * `tokens`: [1, 123]
/// * `memo`: optional message
#[derive(Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Clone))]
#[serde(crate = "near_sdk::serde")]
pub struct SbtRenew {
    pub tokens: Vec<TokenId>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

impl SbtRenew {
    pub fn emit(self) {
        emit_event(Nep393EventKind::SbtRenew(self));
    }
}

/// An event emitted when a existing tokens are revoked.
///
/// Arguments
/// * `tokens`: [1, 123]
/// * `memo`: optional message
#[derive(Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Clone))]
#[serde(crate = "near_sdk::serde")]
pub struct SbtRevoke {
    pub tokens: Vec<TokenId>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

impl SbtRevoke {
    pub fn emit(self) {
        emit_event(Nep393EventKind::SbtRevoke(self));
    }
}

#[cfg(test)]
mod tests {
    use near_contract_standards::non_fungible_token::events::NftMint;
    use near_sdk::{test_utils, AccountId};

    use super::*;

    fn alice() -> AccountId {
        AccountId::new_unchecked("alice.near".to_string())
    }

    fn bob() -> AccountId {
        AccountId::new_unchecked("bob.near".to_string())
    }

    #[test]
    fn log_event_from() {
        let event = Nep393EventKind::SbtMint(vec![SbtMint {
            owner: "bob.near".to_owned(),
            tokens: vec![1, 2],
            memo: None,
        }]);
        let expected = Event {
            standard: STANDARD_NAME.to_owned(),
            version: "1.0.0".to_string(),
            event: event.clone(),
        };
        assert_eq!(expected, event.into());
    }

    #[test]
    fn log_format_mint() {
        let expected = r#"EVENT_JSON:{"standard":"nep393","version":"1.0.0","event":"sbt_mint","data":[{"owner":"bob.near","tokens":[1,2]},{"owner":"user1.near","tokens":[4],"memo":"my memo"}]}"#;
        let event = Nep393EventKind::SbtMint(vec![
            SbtMint {
                owner: "bob.near".to_owned(),
                tokens: vec![1, 2],
                memo: None,
            },
            SbtMint {
                owner: "user1.near".to_owned(),
                tokens: vec![4],
                memo: Some("my memo".to_owned()),
            },
        ]);
        assert_eq!(expected, event.to_json_event_string());

        let event = Nep393EventKind::SbtMint(vec![SbtMint {
            owner: "bob.near".to_owned(),
            tokens: vec![1, 2],
            memo: Some("something".to_owned()),
        }]);

        let token_ids = &["0", "1"];
        let nft_log = NftMint {
            owner_id: &bob(),
            token_ids,
            memo: Some("something"),
        };
        nft_log.emit();
        assert_eq!(test_utils::get_logs()[0], event.to_json_event_string());
    }

    #[test]
    fn log_format_recovery() {
        // "EVENT_JSON:{\"standard\":\"nep393\",\"version\":\"1.0.0\",\"event\":\"sbt_mint\",\"data\":[{\"owner\":\"bob.near\",\"tokens\":[1,2],\"memo\":\"something\"}]}"
        let expected = r#"EVENT_JSON:{"standard":"nep393","version":"1.0.0","event":"sbt_recover","data":[{"old_owner":"user1.near","new_owner":"user2.near","tokens":[10],"memo":"process1"}]}"#;
        let event = Nep393EventKind::SbtRecover(vec![SbtRecover {
            old_owner: "user1.near".to_string(),
            new_owner: "user2.near".to_string(),
            tokens: vec![10],
            memo: Some("process1".to_owned()),
        }]);
        assert_eq!(expected, event.to_json_event_string());
    }
}
