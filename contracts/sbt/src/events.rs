use near_sdk::serde::Serialize;
use near_sdk::{env, AccountId};

use crate::METADATA_SPEC;
use crate::{TokenId, STANDARD_NAME};

/// Enum that represents the data type of the EventLog.
#[derive(Serialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq))]
#[serde(tag = "event", content = "data")]
#[serde(rename_all = "snake_case")]
#[serde(crate = "near_sdk::serde")]
#[non_exhaustive]
pub enum Nep393EventKind<'a> {
    Mint(Vec<SbtMint<'a>>),
    Recover(Vec<SbtRecover<'a>>),
    // no need to use vector of SbtRenew and SbtRevoke events, because the event already has
    // list of token_ids
    Renew(SbtRenew),
    Revoke(SbtRevoke),
}

impl Nep393EventKind<'_> {
    /// creates a string compatible with NEAR event standard
    pub fn to_json_event_string(self) -> String {
        let e = NearEvent {
            standard: STANDARD_NAME,
            version: METADATA_SPEC,
            event: self,
        };
        let s = serde_json::to_string(&e)
            .ok()
            .unwrap_or_else(|| env::abort());
        format!("EVENT_JSON:{}", s)
    }

    pub fn emit(self) {
        env::log_str(&self.to_json_event_string());
    }
}

/// Helper struct to create Standard NEAR Event JSON
///
/// Arguments:
/// * `standard`: name of standard e.g. nep171
/// * `version`: e.g. 1.0.0
/// * `event`: associate event data
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct NearEvent<T: Serialize> {
    pub standard: &'static str,
    pub version: &'static str,

    // `flatten` to not have "event": {<EventLogVariant>} in the JSON, just have the contents of {<EventLogVariant>}.
    #[serde(flatten)]
    pub event: T,
}

/// An event emitted when a new SBT is minted.
///
/// Arguments
/// * `owner`: "account.near"
/// * `tokens`: [1, 123]
/// * `memo`: optional message
#[derive(Serialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Clone))]
#[serde(crate = "near_sdk::serde")]
pub struct SbtMint<'a> {
    pub owner: &'a AccountId,
    pub tokens: Vec<TokenId>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

impl SbtMint<'_> {
    pub fn emit(self) {
        Nep393EventKind::Mint(vec![self]).emit();
    }
}

/// An event emitted when a recovery process succeeded to reassign SBT.
///
/// Arguments
/// * `old_owner`: "owner.near"
/// * `new_owner`: "receiver.near"
/// * `tokens`: [1, 123]
/// * `memo`: optional message
#[derive(Serialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Clone))]
#[serde(crate = "near_sdk::serde")]
pub struct SbtRecover<'a> {
    pub old_owner: &'a AccountId,
    pub new_owner: &'a AccountId,
    pub tokens: Vec<TokenId>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

impl SbtRecover<'_> {
    pub fn emit(self) {
        Nep393EventKind::Recover(vec![self]).emit();
    }
}

/// An event emitted when a existing tokens are renewed.
///
/// Arguments
/// * `tokens`: [1, 123]
/// * `memo`: optional message
#[derive(Serialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Clone))]
#[serde(crate = "near_sdk::serde")]
pub struct SbtRenew {
    pub tokens: Vec<TokenId>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

impl SbtRenew {
    pub fn emit(self) {
        Nep393EventKind::Renew(self).emit();
    }
}

/// An event emitted when a existing tokens are revoked.
///
/// Arguments
/// * `tokens`: [1, 123]
/// * `memo`: optional message
#[derive(Serialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Clone))]
#[serde(crate = "near_sdk::serde")]
pub struct SbtRevoke {
    pub tokens: Vec<TokenId>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

impl SbtRevoke {
    pub fn emit(self) {
        Nep393EventKind::Revoke(self).emit();
    }
}

#[cfg(test)]
mod tests {
    use near_contract_standards::non_fungible_token::events::NftMint;
    use near_sdk::test_utils;

    use super::*;

    fn alice() -> AccountId {
        AccountId::new_unchecked("alice.near".to_string())
    }

    fn bob() -> AccountId {
        AccountId::new_unchecked("bob.near".to_string())
    }

    #[test]
    fn log_format_mint() {
        let alice = alice();
        let bob = bob();
        let expected = r#"EVENT_JSON:{"standard":"nep393","version":"1.0.0","event":"mint","data":[{"owner":"bob.near","tokens":[1,2]},{"owner":"alice.near","tokens":[4],"memo":"my memo"}]}"#;
        let event = Nep393EventKind::Mint(vec![
            SbtMint {
                owner: &bob,
                tokens: vec![1, 2],
                memo: None,
            },
            SbtMint {
                owner: &alice,
                tokens: vec![4],
                memo: Some("my memo".to_owned()),
            },
        ]);
        assert_eq!(expected, event.to_json_event_string());

        let event = Nep393EventKind::Mint(vec![SbtMint {
            owner: &bob,
            tokens: vec![1, 2],
            memo: Some("something".to_owned()),
        }]);

        let token_ids = &["0", "1"];
        let nft_log = NftMint {
            owner_id: &bob,
            token_ids,
            memo: Some("something"),
        };
        nft_log.emit();
        // TODO: fix
        assert_ne!(test_utils::get_logs()[0], event.to_json_event_string());
    }

    #[test]
    fn log_format_recovery() {
        let alice = alice();
        let bob = bob();
        // "EVENT_JSON:{\"standard\":\"nep393\",\"version\":\"1.0.0\",\"event\":\"sbt_mint\",\"data\":[{\"owner\":\"bob.near\",\"tokens\":[1,2],\"memo\":\"something\"}]}"
        let expected = r#"EVENT_JSON:{"standard":"nep393","version":"1.0.0","event":"recover","data":[{"old_owner":"alice.near","new_owner":"bob.near","tokens":[10],"memo":"process1"}]}"#;
        let event = Nep393EventKind::Recover(vec![SbtRecover {
            old_owner: &alice,
            new_owner: &bob,
            tokens: vec![10],
            memo: Some("process1".to_owned()),
        }]);
        assert_eq!(expected, event.to_json_event_string());
    }
}
