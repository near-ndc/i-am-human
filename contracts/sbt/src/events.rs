use near_sdk::serde::Serialize;
use near_sdk::{env, AccountId};

use crate::SPEC_VERSION;
use crate::{TokenId, STANDARD_NAME};

/// Helper struct to create Standard NEAR Event JSON.
/// Arguments:
/// * `standard`: name of standard e.g. nep171
/// * `version`: e.g. 1.0.0
/// * `event`: associate event data
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct NearEvent<T: Serialize> {
    pub standard: &'static str,
    pub version: &'static str,

    // `flatten` to not have "event": {<EventVariant>} in the JSON, just have the contents of {<EventVariant>}.
    #[serde(flatten)]
    pub event: T,
}

/// Enum that represents the data type of a NEP-393 Event.
#[derive(Serialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Clone))]
#[serde(tag = "event", content = "data")]
#[serde(rename_all = "snake_case")]
#[serde(crate = "near_sdk::serde")]
#[non_exhaustive]
pub enum Nep393Event<'a> {
    // no need to use vector for the event types below, because an event already has list of token IDs
    Mint(SbtMint<'a>),
    Recover(SbtRecover<'a>),
    Renew(SbtTokensEvent),
    Revoke(SbtTokensEvent),
    Burn(SbtTokensEvent),
    SoulTransfer(SoulTransfer<'a>),
    Ban(Vec<&'a AccountId>), // data is a simple list of accounts to ban
}

impl Nep393Event<'_> {
    /// creates a string compatible with NEAR event standard
    pub fn to_json_event_string(self) -> String {
        let e = NearEvent {
            standard: STANDARD_NAME,
            version: SPEC_VERSION,
            event: self,
        };
        let s = serde_json::to_string(&e)
            .ok()
            .unwrap_or_else(|| env::abort());
        format!("EVENT_JSON:{}", s)
    }

    // todo: maybe move to NearEvent
    pub fn emit(self) {
        env::log_str(&self.to_json_event_string());
    }
}

/// An event emitted when an SBT token issuance succeeded.
/// Arguments:
/// * `issuer`: SBT smart contract initiating the token issuance.
/// * `tokens`: list of pairs (token owner, TokenId[])
/// * `memo`: optional message
#[derive(Serialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Clone))]
#[serde(crate = "near_sdk::serde")]
pub struct SbtMint<'a> {
    pub issuer: &'a AccountId,
    pub tokens: Vec<(&'a AccountId, &'a Vec<TokenId>)>,
}
impl SbtMint<'_> {
    pub fn emit(self) {
        Nep393Event::Mint(self).emit();
    }
}

/// An event emitted when a recovery process succeeded to reassign SBT, usually due to account
/// access loss. This action is usually requested by the owner, but executed by an issuer,
/// and doesn't trigger Soul Transfer. Registry recovers all tokens assigned to `old_owner`,
/// hence we don't need to enumerate them.
/// Must be emitted by an SBT registry.
/// * `issuer`: SBT smart contract initiating the token recovery.
/// * `old_owner`: source account from which we recover the tokens.
/// * `new_owner`: destination account for recevered tokens.
#[derive(Serialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Clone))]
#[serde(crate = "near_sdk::serde")]
pub struct SbtRecover<'a> {
    pub issuer: &'a AccountId,
    pub old_owner: &'a AccountId,
    pub new_owner: &'a AccountId,
}

impl SbtRecover<'_> {
    pub fn emit(self) {
        Nep393Event::Recover(self).emit();
    }
}

/// A common structure for the following events:
/// renew, revoke, burn.
/// Arguments:
/// * `issuer`: SBT smart contract initiating the SBT state change.
/// * `tokens`: list of tokens concering the transaction emitting the event.
#[derive(Serialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Clone))]
#[serde(crate = "near_sdk::serde")]
pub struct SbtTokensEvent {
    pub issuer: AccountId, // SBT Contract account address
    pub tokens: Vec<TokenId>,
}

impl SbtTokensEvent {
    pub fn emit_renew(self) {
        Nep393Event::Renew(self).emit();
    }

    pub fn emit_revoke(self) {
        Nep393Event::Revoke(self).emit();
    }

    pub fn emit_burn(self) {
        Nep393Event::Burn(self).emit();
    }
}

/// An event emitted when soul transfer is happening: all SBTs owned by `from` are transferred
/// to `to`, and the `from` account is banned (can't receive any new SBT).
/// Must be emitted by an SBT registry.
/// Registry MUST also emit `Ban` whenever the soul transfer happens.
#[derive(Serialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Clone))]
#[serde(crate = "near_sdk::serde")]
pub struct SoulTransfer<'a> {
    pub from: &'a AccountId,
    pub to: &'a AccountId,
}

impl SoulTransfer<'_> {
    pub fn emit(self) {
        Nep393Event::SoulTransfer(self).emit();
    }
}

pub fn emit_soul_transfer(from: &AccountId, to: &AccountId) {
    SoulTransfer { from, to }.emit();
}

/// Helper function to be used with `NearEvent` to construct NAER Event compatible payload
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
struct EventWrapper<T: Serialize> {
    event: &'static str,
    data: T,
}

/// NEP-171 compatible Mint event structure. A light version of the Mint event from the
/// `near_contract_standards::non_fungible_token::events::NftMint` to reduce code dependency and size.
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Nep171Mint<'a> {
    pub owner_id: &'a AccountId,
    pub token_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

impl Nep171Mint<'_> {
    pub fn many_to_json_event_string(data: &[Nep171Mint<'_>]) -> String {
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

    pub fn emit_many(data: &[Nep171Mint<'_>]) {
        env::log_str(&Nep171Mint::many_to_json_event_string(data));
    }

    /// creates a string compatible NEP-171 NftMint event standard.
    pub fn to_json_event_string(self) -> String {
        Nep171Mint::many_to_json_event_string(&[self])
    }

    pub fn emit(self) {
        env::log_str(&self.to_json_event_string());
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

    fn charlie() -> AccountId {
        AccountId::new_unchecked("charlie.near".to_string())
    }

    fn sbt_issuer() -> AccountId {
        AccountId::new_unchecked("sbt.near".to_string())
    }

    fn from_nftmint<'a>(n: &NftMint<'a>) -> Nep171Mint<'a> {
        Nep171Mint {
            owner_id: n.owner_id,
            token_ids: n.token_ids.iter().map(|s| s.clone().to_owned()).collect(),
            memo: n.memo.map(|s| s.to_owned()),
        }
    }

    #[test]
    fn log_format_nep171_mint() {
        let alice = alice();
        let bob = bob();
        let expected = r#"EVENT_JSON:{"standard":"nep171","version":"1.0.0","event":"nft_mint","data":[{"owner_id":"bob.near","token_ids":["0","1"]},{"owner_id":"alice.near","token_ids":["4"],"memo":"something"}]}"#;
        let nft_log = vec![
            NftMint {
                owner_id: &bob,
                token_ids: &["0", "1"],
                memo: None,
            },
            NftMint {
                owner_id: &alice,
                token_ids: &["4"],
                memo: Some("something"),
            },
        ];
        NftMint::emit_many(&nft_log);
        assert_eq!(1, test_utils::get_logs().len());
        assert_eq!(expected, test_utils::get_logs()[0]);

        let sbt_log: Vec<Nep171Mint> = nft_log.iter().map(from_nftmint).collect();
        assert_eq!(expected, Nep171Mint::many_to_json_event_string(&sbt_log));

        Nep171Mint::emit_many(&sbt_log);
        assert_eq!(2, test_utils::get_logs().len());
        assert_eq!(test_utils::get_logs()[1], expected);

        //
        // Check single event log
        //
        let expected = r#"EVENT_JSON:{"standard":"nep171","version":"1.0.0","event":"nft_mint","data":[{"owner_id":"alice.near","token_ids":["1123"],"memo":"something"}]}"#;
        let nft_log = NftMint {
            owner_id: &alice,
            token_ids: &["1123"],
            memo: Some("something"),
        };
        let log = from_nftmint(&nft_log);
        let log2 = from_nftmint(&nft_log);

        nft_log.emit();
        assert_eq!(3, test_utils::get_logs().len());
        assert_eq!(expected, test_utils::get_logs()[2]);

        log.emit();
        assert_eq!(4, test_utils::get_logs().len());
        assert_eq!(expected, test_utils::get_logs()[3]);
        assert_eq!(expected, log2.to_json_event_string());
    }

    #[test]
    fn log_format_mint() {
        let bob = bob();
        let issuer = sbt_issuer();
        let expected = r#"EVENT_JSON:{"standard":"nep393","version":"1.0.0","event":"mint","data":{"issuer":"sbt.near","tokens":[["bob.near",[821,10]],["bob.near",[1]]]}}"#;
        let bob1_tokens = vec![821, 10];
        let bob2_tokens = vec![1];
        let event = Nep393Event::Mint(SbtMint {
            issuer: &issuer,
            tokens: vec![(&bob, &bob1_tokens), (&bob, &bob2_tokens)],
        });
        assert_eq!(expected, event.clone().to_json_event_string());
        event.emit();
        assert_eq!(expected, test_utils::get_logs()[0]);
    }

    #[test]
    fn log_format_recovery() {
        let bob = bob();
        let charlie = charlie();
        let issuer = sbt_issuer();
        let expected = r#"EVENT_JSON:{"standard":"nep393","version":"1.0.0","event":"recover","data":{"issuer":"sbt.near","old_owner":"bob.near","new_owner":"charlie.near"}}"#;
        let event = Nep393Event::Recover(SbtRecover {
            issuer: &issuer,
            old_owner: &bob,
            new_owner: &charlie,
        });
        assert_eq!(expected, event.clone().to_json_event_string());
        event.emit();
        assert_eq!(expected, test_utils::get_logs()[0]);
    }

    #[test]
    fn log_format_renew() {
        let expected = r#"EVENT_JSON:{"standard":"nep393","version":"1.0.0","event":"renew","data":{"issuer":"sbt.near","tokens":[21,10,888]}}"#;
        let e = SbtTokensEvent {
            issuer: sbt_issuer(),
            tokens: vec![21, 10, 888],
        };
        let event = Nep393Event::Renew(e.clone());
        assert_eq!(expected, event.clone().to_json_event_string());
        event.emit();
        assert_eq!(expected, test_utils::get_logs()[0]);
        e.emit_renew();
        assert_eq!(expected, test_utils::get_logs()[1]);
    }

    #[test]
    fn log_format_revoke() {
        let expected = r#"EVENT_JSON:{"standard":"nep393","version":"1.0.0","event":"revoke","data":{"issuer":"sbt.near","tokens":[19853,1]}}"#;
        let e = SbtTokensEvent {
            issuer: sbt_issuer(),
            tokens: vec![19853, 1],
        };
        let event = Nep393Event::Revoke(e.clone());
        assert_eq!(expected, event.clone().to_json_event_string());
        event.emit();
        assert_eq!(expected, test_utils::get_logs()[0]);
        e.emit_revoke();
        assert_eq!(expected, test_utils::get_logs()[1]);
    }

    #[test]
    fn log_format_burn() {
        let expected = r#"EVENT_JSON:{"standard":"nep393","version":"1.0.0","event":"burn","data":{"issuer":"sbt.near","tokens":[19853,12]}}"#;
        let e = SbtTokensEvent {
            issuer: sbt_issuer(),
            tokens: vec![19853, 12],
        };
        let event = Nep393Event::Burn(e.clone());
        assert_eq!(expected, event.clone().to_json_event_string());
        event.emit();
        assert_eq!(expected, test_utils::get_logs()[0]);
        e.emit_burn();
        assert_eq!(expected, test_utils::get_logs()[1]);
    }

    #[test]
    fn log_format_ban() {
        let alice = alice();
        let bob = bob();
        let expected = r#"EVENT_JSON:{"standard":"nep393","version":"1.0.0","event":"ban","data":["alice.near","bob.near"]}"#;
        let event = Nep393Event::Ban(vec![&alice, &bob]);
        assert_eq!(expected, event.clone().to_json_event_string());
        event.emit();
        assert_eq!(expected, test_utils::get_logs()[0]);
    }

    #[test]
    fn log_soul_transfer() {
        let alice = alice();
        let bob = bob();
        let expected = r#"EVENT_JSON:{"standard":"nep393","version":"1.0.0","event":"soul_transfer","data":{"from":"alice.near","to":"bob.near"}}"#;
        let e = SoulTransfer {
            from: &alice,
            to: &bob,
        };
        let event = Nep393Event::SoulTransfer(e.clone());
        assert_eq!(expected, event.clone().to_json_event_string());
        event.emit();
        assert_eq!(expected, test_utils::get_logs()[0]);
        e.emit();
        assert_eq!(expected, test_utils::get_logs()[1]);
    }
}
