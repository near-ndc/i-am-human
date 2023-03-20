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

/// Helper function to be used with `NearEvent` to construct NAER Event compatible payload
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
struct EventWrapper<T: Serialize> {
    event: &'static str,
    data: T,
}

/// NEP-171 compatible Mint event structure.
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Mint<'a> {
    pub owner_id: &'a AccountId,
    pub token_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

impl Mint<'_> {
    pub fn many_to_json_event_string(data: &[Mint<'_>]) -> String {
        let e = NearEvent {
            standard: "nep171",
            version: "1.0.0",
            event: EventWrapper {
                event: "nft_mint",
                data,
            },
        };
        let s = serde_json::to_string(&e)
            .ok()
            .unwrap_or_else(|| env::abort());
        format!("EVENT_JSON:{}", s)
    }

    pub fn emit_many(data: &[Mint<'_>]) {
        env::log_str(&Mint::many_to_json_event_string(data));
    }

    /// creates a string compatible NEP-171 NftMint event standard.
    pub fn to_json_event_string(self) -> String {
        Mint::many_to_json_event_string(&[self])
    }

    pub fn emit(self) {
        env::log_str(&self.to_json_event_string());
    }
}

/// Helper function to create Mint event end emit it.
pub fn emit_mint_event(owner_id: &AccountId, token: TokenId, memo: Option<String>) {
    Mint {
        owner_id,
        token_ids: vec![token.to_string()],
        memo,
    }
    .emit()
}

#[cfg(test)]
mod tests {
    use near_contract_standards::non_fungible_token::events::NftMint as Nep171Mint;
    use near_sdk::test_utils;

    use super::*;

    fn alice() -> AccountId {
        AccountId::new_unchecked("alice.near".to_string())
    }

    fn bob() -> AccountId {
        AccountId::new_unchecked("bob.near".to_string())
    }

    fn nft_to_sbt_mint<'a>(n: &Nep171Mint<'a>) -> Mint<'a> {
        Mint {
            owner_id: n.owner_id,
            token_ids: n.token_ids.iter().map(|s| s.clone().to_owned()).collect(),
            memo: n.memo.map(|s| s.to_owned()),
        }
    }

    #[test]
    fn log_format_mint() {
        let alice = alice();
        let bob = bob();
        let expected = r#"EVENT_JSON:{"standard":"nep171","version":"1.0.0","event":"nft_mint","data":[{"owner_id":"bob.near","token_ids":["0","1"]},{"owner_id":"alice.near","token_ids":["4"],"memo":"something"}]}"#;
        let nft_log = vec![
            Nep171Mint {
                owner_id: &bob,
                token_ids: &["0", "1"],
                memo: None,
            },
            Nep171Mint {
                owner_id: &alice,
                token_ids: &["4"],
                memo: Some("something"),
            },
        ];
        Nep171Mint::emit_many(&nft_log);
        assert_eq!(1, test_utils::get_logs().len());
        assert_eq!(expected, test_utils::get_logs()[0]);

        let sbt_log: Vec<Mint> = nft_log.iter().map(nft_to_sbt_mint).collect();
        assert_eq!(expected, Mint::many_to_json_event_string(&sbt_log));

        Mint::emit_many(&sbt_log);
        assert_eq!(2, test_utils::get_logs().len());
        assert_eq!(test_utils::get_logs()[1], expected);

        //
        // Check single event log
        //
        let expected = r#"EVENT_JSON:{"standard":"nep171","version":"1.0.0","event":"nft_mint","data":[{"owner_id":"alice.near","token_ids":["1123"],"memo":"something"}]}"#;
        let nft_log = Nep171Mint {
            owner_id: &alice,
            token_ids: &["1123"],
            memo: Some("something"),
        };
        let sbt_log = nft_to_sbt_mint(&nft_log);
        let sbt_log2 = nft_to_sbt_mint(&nft_log);
        emit_mint_event(nft_log.owner_id, 1123, sbt_log.memo.clone());
        assert_eq!(3, test_utils::get_logs().len());
        assert_eq!(expected, test_utils::get_logs()[2]);

        nft_log.emit();
        assert_eq!(4, test_utils::get_logs().len());
        assert_eq!(expected, test_utils::get_logs()[3]);

        sbt_log.emit();
        assert_eq!(5, test_utils::get_logs().len());
        assert_eq!(expected, test_utils::get_logs()[4]);
        assert_eq!(expected, sbt_log2.to_json_event_string());
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
        // assert_ne!(test_utils::get_logs()[0], event.to_json_event_string());
    }
}
