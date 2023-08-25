use crate::consts::PROOF_OF_KUDOS_SBT_CLASS_ID;
use crate::registry::TokenMetadata;
use crate::types::KudosId;
use crate::{CommentId, EncodedCommentary, Hashtag, KudosKind, WrappedCid};
use near_sdk::env::STORAGE_PRICE_PER_BYTE;
use near_sdk::serde_json::{self, Value};
use near_sdk::{AccountId, Balance, Gas};

/// Return initial object as JSON [`String`] which will be stored in NEAR social db
///
/// Example of JSON output:
/// ```json
/// {
///   "kudos.near": {
///     "kudos": {},
///     "hashtags": {}
///   }
/// }
/// ```
///
/// ATTENTION: Changing this JSON output will require contract refactoring and re-computation
/// of deposit requirement for all public methods
pub fn build_initial_json_for_socialdb(root_id: &AccountId) -> Result<Value, &'static str> {
    serde_json::from_str::<Value>(&format!(
        r#"{{
          "{root_id}": {{
            "kudos": {{}},
            "hashtags": {{}}
          }}
        }}"#
    ))
    .map_err(|_| "Internal serialization error")
}

/// Return hashtags relationship to kudos and it's owner as JSON [`String`] which will be stored in NEAR social db
///
/// Example of JSON output:
/// ```json
/// {
///   "lovendc": {
///     "1": "ndc.near",
///     ...
///   },
///   ...
/// }
/// ```
///
/// ATTENTION: Changing this JSON output will require contract refactoring and re-computation
/// of deposit requirement for public method [`give_kudos`](kudos_contract::public::Contract::give_kudos)
pub fn build_hashtags(
    receiver_id: &AccountId,
    kudos_id: &KudosId,
    hashtags: Option<&[Hashtag]>,
) -> Result<String, &'static str> {
    hashtags
        .map(|hashtags| {
            hashtags
                .iter()
                .map(|ht| {
                    serde_json::from_str::<Value>(&format!(
                        r#"{{
                          "{kudos_id}": "{receiver_id}"
                        }}"#
                    ))
                    .map(|v| (ht, v))
                })
                .collect::<Result<std::collections::BTreeMap<_, _>, _>>()
                .and_then(|map| serde_json::to_string(&map))
                .map_err(|_| "Internal serialization error")
        })
        .unwrap_or_else(|| Ok("{}".to_owned()))
}

/// Return hashtags array as JSON [`String`] which will be stored in NEAR social db
///
/// Example of JSON output:
/// ```json
/// [
///   "nearcommunity",
///   "ndckudos",
///   ...
/// ]
/// ```
///
/// ATTENTION: Changing this JSON output will require contract refactoring and re-computation
/// of deposit requirement for public method [`give_kudos`](kudos_contract::public::Contract::give_kudos)
pub fn hashtags_to_json_array(hashtags: &[Hashtag]) -> Result<String, &'static str> {
    serde_json::to_string(&hashtags)
        .map(|s| s.escape_default().to_string())
        .map_err(|_| "Internal hashtags serialization error")
}

/// Return kudos object as JSON [`String`] which will be stored in NEAR social db
///
/// Example of JSON output:
/// ```json
/// {
///   "kudos.near": {
///     "kudos": {
///       "some_user.near": {
///         "1": {
///           "created_at": "1689976833613",
///           "sender_id": "alex.near",
///           "kind": "k",
///           "message": "that user is awesome",
///           "icon": "bafybeigrf2dwtpjkiovnigysyto3d55opf6qkdikx6d65onrqnfzwgdkfa",
///           "upvotes": {},
///           "comments": {},
///           "tags": "[\"firstkudos\",\"awesomework\"]",
///         }
///       }
///     },
///     "hashtags": {
///       "firstkudos": {
///         "1": "alex.near"
///       },
///       "awesomework": {
///         "1": "alex.near"
///       }
///     }
///   }
/// }
/// ```
///
/// ATTENTION: Changing this JSON output will require contract refactoring and re-computation
/// of deposit requirement for public method [`give_kudos`](kudos_contract::public::Contract::give_kudos)
pub fn build_give_kudos_request(
    root_id: &AccountId,
    sender_id: &AccountId,
    receiver_id: &AccountId,
    kudos_id: &KudosId,
    created_at: u64,
    kind: KudosKind,
    message: &str,
    icon_cid: Option<&WrappedCid>,
    hashtags: Option<&[Hashtag]>,
) -> Result<Value, &'static str> {
    let hashtags_as_array_json = hashtags_to_json_array(hashtags.unwrap_or(&[]))?;
    let hashtags_with_kudos = build_hashtags(receiver_id, kudos_id, hashtags)?;
    let icon_cid = icon_cid.map(|cid| cid.to_string()).unwrap_or_default();

    let mes = near_sdk::serde_json::Value::String(message.to_string());
    serde_json::from_str::<Value>(&format!(
        r#"{{
          "{root_id}": {{
            "kudos": {{
              "{receiver_id}": {{
                "{kudos_id}": {{
                  "created_at": "{created_at}",
                  "sender_id": "{sender_id}",
                  "kind": "{kind}",
                  "message": {mes},
                  "icon": "{icon_cid}",
                  "upvotes": {{}},
                  "comments": {{}},
                  "tags": "{hashtags_as_array_json}"
                }}
              }}
            }},
            "hashtags": {hashtags_with_kudos}
          }}
        }}"#
    ))
    .map_err(|e| {
        println!("{e:?}");
        "Internal serialization error"
    })
}

/// Return upvotes for kudos object as JSON [`String`] which will be stored in NEAR social db
///
/// Example of JSON output:
/// ```json
/// {
///   "kudos.near": {
///     "kudos": {
///       "some_user.near": {
///         "1": {
///           "upvotes": {
///             "bob.near": ""
///           }
///         }
///       }
///     }
///   }
/// }
/// ```
///
/// ATTENTION: Changing this JSON output will require contract refactoring and re-computation
/// of deposit requirement for public method [`upvote_kudos`](kudos_contract::public::Contract::upvote_kudos)
pub fn build_upvote_kudos_request(
    root_id: &AccountId,
    sender_id: &AccountId,
    receiver_id: &AccountId,
    kudos_id: &KudosId,
) -> Result<Value, &'static str> {
    serde_json::from_str::<Value>(&format!(
        r#"{{
          "{root_id}": {{
            "kudos": {{
              "{receiver_id}": {{
                "{kudos_id}": {{
                  "upvotes": {{
                    "{sender_id}": ""
                  }}
                }}
              }}
            }}
          }}
        }}"#
    ))
    .map_err(|_| "Internal serialization error")
}

/// Return base64-encoded commentary for kudos object as JSON [`String`] which will be stored in NEAR social db
///
/// Example of JSON output:
/// ```json
/// {
///   "kudos.near": {
///     "kudos": {
///       "some_user.near": {
///         "1": {
///           "comments": {
///             "2": "eyJtIjoiY29tbWVudGFyeSB0ZXN0IiwicyI6InVzZXIubmVhciIsInQiOiIxMjM0NTY3ODkwIn0="
///           }
///         }
///       }
///     }
///   }
/// }
/// ```
///
/// ATTENTION: Changing this JSON output will require contract refactoring and re-computation
/// of deposit requirement for public method [`leave_comment`](kudos_contract::public::Contract::leave_comment)
pub fn build_leave_comment_request(
    root_id: &AccountId,
    receiver_id: &AccountId,
    kudos_id: &KudosId,
    comment_id: &CommentId,
    comment: &EncodedCommentary,
) -> Result<Value, &'static str> {
    let comment = comment.as_str();
    let json = format!(
        r#"{{
          "{root_id}": {{
            "kudos": {{
              "{receiver_id}": {{
                "{kudos_id}": {{
                  "comments": {{
                    "{comment_id}": "{comment}"
                  }}
                }}
              }}
            }}
          }}
        }}"#
    );
    serde_json::from_str::<Value>(&json).map_err(|_| "Internal serialization error")
}

/// Return [`String`] path to a stored kudos JSON with unique [`KudosId`] for a valid [`AccountId`]
/// used to query from NEAR social db.
///
/// Example of query: "kudos.near/kudos/alex.near/1/*"
pub fn build_get_kudos_by_id_request(
    root_id: &AccountId,
    receiver_id: &AccountId,
    kudos_id: &KudosId,
) -> String {
    format!("{root_id}/kudos/{receiver_id}/{kudos_id}/*")
}

/// Return [`String`] path to a stored kudos base64-encoded comment with unique [`KudosId`] and [`CommentId`]
/// for a valid [`AccountId`] used to query from NEAR social db.
///
/// Example of query: "kudos.near/kudos/alex.near/1/comments/2"
pub fn build_get_kudos_comment_by_id_request(
    root_id: &AccountId,
    receiver_id: &AccountId,
    kudos_id: &KudosId,
    comment_id: &CommentId,
) -> String {
    format!("{root_id}/kudos/{receiver_id}/{kudos_id}/comments/{comment_id}")
}

/// Return [`String`] path to a stored upvotes information JSON with unique [`KudosId`] for a valid [`AccountId`]
/// used to query from NEAR social db.
///
/// Example of query: "kudos.near/kudos/alex.near/1/upvotes"
pub fn build_kudos_upvotes_path(
    root_id: &AccountId,
    receiver_id: &AccountId,
    kudos_id: &KudosId,
) -> String {
    format!("{root_id}/kudos/{receiver_id}/{kudos_id}/upvotes")
}

/// Return [`String`] path to a stored kudos kind type with unique [`KudosId`] for a valid [`AccountId`]
/// used to query from NEAR social db.
///
/// Example of query: "kudos.near/kudos/bob.near/1/kind"
pub fn build_kudos_kind_path(
    root_id: &AccountId,
    receiver_id: &AccountId,
    kudos_id: &KudosId,
) -> String {
    format!("{root_id}/kudos/{receiver_id}/{kudos_id}/kind")
}

/// Return [`TokenMetadata`] used as an argument for call [`sbt_mint`](kudos_contract::registry::ExtSbtRegistry::sbt_mint)
/// to mint ProofOfKudos SBT
pub fn build_pok_sbt_metadata(issued_at: u64, expires_at: u64) -> TokenMetadata {
    TokenMetadata {
        class: PROOF_OF_KUDOS_SBT_CLASS_ID,
        issued_at: Some(issued_at),
        expires_at: Some(expires_at),
        reference: None,
        reference_hash: None,
    }
}

/// Extract sender [`AccountId`] from stored kudos JSON acquired from NEAR social db
pub fn extract_kudos_id_sender_from_response(req: &str, res: &mut Value) -> Option<AccountId> {
    remove_key_from_json(res, &req.replace('*', "sender_id"))
        .and_then(|val| serde_json::from_value::<AccountId>(val).ok())
}

/// Extract kudos base64-encoded comment [`EncodedCommentary`] by [`CommentId`] from stored kudos JSON acquired from NEAR social db
pub fn extract_kudos_encoded_comment_by_id_from_response(
    req: &str,
    comment_id: &CommentId,
    res: &mut Value,
) -> Option<EncodedCommentary> {
    remove_key_from_json(res, &req.replace('*', &format!("comments/{comment_id}")))
        .and_then(|val| serde_json::from_value::<EncodedCommentary>(val).ok())
}

/// Remove and return (if removed) [`serde_json::Value`] by key name [`str`] from JSON [`serde_json::Value`]
///
/// # Example:
/// ```
/// use kudos_contract::utils::remove_key_from_json;
/// use near_sdk::serde_json;
///
/// let mut initial_value = serde_json::json!({
///   "key1": {
///     "key2": {
///       "key3": {
///         "key4": "value"
///       }
///     }
///   }
/// });
/// let removed_value = remove_key_from_json(&mut initial_value, "key1/key2/key3");
/// assert_eq!(
///     initial_value,
///     serde_json::json!({
///       "key1": {
///         "key2": {}
///       }
///     })
/// );
/// assert_eq!(
///     removed_value,
///     Some(serde_json::json!({
///       "key4": "value"
///     }))
/// );
/// ```
pub fn remove_key_from_json(json: &mut Value, key: &str) -> Option<Value> {
    let mut json = Some(json);
    let mut keys = key.split('/').peekable();

    while let Some(key) = keys.next() {
        match json {
            Some(Value::Object(obj)) if keys.peek().is_none() => {
                return obj.remove(key);
            }
            Some(Value::Object(obj)) => json = obj.get_mut(key),
            _ => break,
        }
    }

    None
}

/// Checks if provided value of type T is equal to T::default()
// pub(crate) fn is_default<T: Default + PartialEq>(t: &T) -> bool {
//     t == &T::default()
// }

pub(crate) fn opt_default<T>() -> Option<T> {
    Option::<T>::None
}

/// Return [`String`] message which represents human-readable attached TGas requirements for a call
pub(crate) fn display_gas_requirement_in_tgas(gas: Gas) -> String {
    format!(
        "Requires minimum amount of attached gas {} TGas",
        gas.0 / Gas::ONE_TERA.0
    )
}

/// Return [`String`] message which represents human-readable attached Ⓝ deposit requirements for a call
pub(crate) fn display_deposit_requirement_in_near(value: Balance) -> String {
    format!(
        "Requires exact amount of attached deposit {} NEAR",
        (value / STORAGE_PRICE_PER_BYTE) as f64 / 100_000f64
    )
}

/// Return [`String`] which represents human-readable Ⓝ amount
pub fn display_deposit_in_near(value: Balance) -> String {
    format!(
        "{} NEAR",
        (value / STORAGE_PRICE_PER_BYTE) as f64 / 100_000f64
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EncodedCommentary;
    use crate::{types::IncrementalUniqueId, Commentary};
    use near_sdk::json_types::U64;
    use near_sdk::serde_json::json;
    use near_units::parse_near;

    #[test]
    fn test_build_initial_json_for_socialdb() {
        let root_id = AccountId::new_unchecked("kudos.near".to_owned());

        let json_text = super::build_initial_json_for_socialdb(&root_id).unwrap();
        assert_eq!(
            json_text,
            json!({
                "kudos.near": {
                  "kudos": {},
                  "hashtags": {}
                }
            })
        )
    }

    #[test]
    fn test_build_hashtags() {
        let receiver_id = AccountId::new_unchecked("test2.near".to_owned());
        let next_kudos_id = KudosId::from(IncrementalUniqueId::default().next());

        let json_text = super::build_hashtags(
            &receiver_id,
            &next_kudos_id,
            Some(&[Hashtag::new("hashtaga", 32).unwrap(),
                Hashtag::new("hashtagb", 32).unwrap(),
                Hashtag::new("hashtagc", 32).unwrap()]),
        )
        .unwrap();

        assert_eq!(
            json_text,
            r#"{"hashtaga":{"1":"test2.near"},"hashtagb":{"1":"test2.near"},"hashtagc":{"1":"test2.near"}}"#
        );
    }

    #[test]
    fn test_hashtags_to_json_array() {
        assert_eq!(
            hashtags_to_json_array(&[
                Hashtag::new("a1", 32).unwrap(),
                Hashtag::new("b1", 32).unwrap(),
                Hashtag::new("c1", 32).unwrap(),
            ])
            .unwrap(),
            r#"[\"a1\",\"b1\",\"c1\"]"#
        );
        assert_eq!(hashtags_to_json_array(&[]).unwrap(), r#"[]"#);
    }

    #[test]
    fn test_build_kudos_request() {
        let root_id = AccountId::new_unchecked("kudos.near".to_owned());
        let sender_id = AccountId::new_unchecked("test1.near".to_owned());
        let receiver_id = AccountId::new_unchecked("test2.near".to_owned());
        let next_kudos_id = KudosId::from(IncrementalUniqueId::default().next());
        //let message = EscapedMessage::new(r#""a","b":{"t":"multi\nline"},"#, 1000).unwrap();
        let message = r#""a","b":{"t":"multi\nline"},"#;
        let icon_cid =
            WrappedCid::new("bafybeigrf2dwtpjkiovnigysyto3d55opf6qkdikx6d65onrqnfzwgdkfa").unwrap();

        let json_text = serde_json::to_string(
            &super::build_give_kudos_request(
                &root_id,
                &sender_id,
                &receiver_id,
                &next_kudos_id,
                1234567890u64,
                KudosKind::Kudos,
                message,
                Some(&icon_cid),
                Some(&[
                    Hashtag::new("abc", 32).unwrap(),
                    Hashtag::new("def", 32).unwrap(),
                ]),
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            json_text,
            r#"{"kudos.near":{"hashtags":{"abc":{"1":"test2.near"},"def":{"1":"test2.near"}},"kudos":{"test2.near":{"1":{"comments":{},"created_at":"1234567890","icon":"bafybeigrf2dwtpjkiovnigysyto3d55opf6qkdikx6d65onrqnfzwgdkfa","kind":"k","message":"\"a\",\"b\":{\"t\":\"multi\\nline\"},","sender_id":"test1.near","tags":"[\"abc\",\"def\"]","upvotes":{}}}}}}"#
        );

        let json_text = serde_json::to_string(
            &super::build_give_kudos_request(
                &root_id,
                &sender_id,
                &receiver_id,
                &next_kudos_id,
                1234567890u64,
                KudosKind::Ding,
                message,
                None,
                Some(&[
                    Hashtag::new("abc", 32).unwrap(),
                    Hashtag::new("def", 32).unwrap(),
                ]),
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            json_text,
            r#"{"kudos.near":{"hashtags":{"abc":{"1":"test2.near"},"def":{"1":"test2.near"}},"kudos":{"test2.near":{"1":{"comments":{},"created_at":"1234567890","icon":"","kind":"d","message":"\"a\",\"b\":{\"t\":\"multi\\nline\"},","sender_id":"test1.near","tags":"[\"abc\",\"def\"]","upvotes":{}}}}}}"#
        );
    }

    #[test]
    fn test_build_upvote_kudos_request() {
        let root_id = AccountId::new_unchecked("kudos.near".to_owned());
        let sender_id = AccountId::new_unchecked("test1.near".to_owned());
        let receiver_id = AccountId::new_unchecked("test2.near".to_owned());
        let next_kudos_id = KudosId::from(IncrementalUniqueId::default().next());

        let json_text = serde_json::to_string(
            &super::build_upvote_kudos_request(&root_id, &sender_id, &receiver_id, &next_kudos_id)
                .unwrap(),
        )
        .unwrap();

        assert_eq!(
            json_text,
            r#"{"kudos.near":{"kudos":{"test2.near":{"1":{"upvotes":{"test1.near":""}}}}}}"#
        );
    }

    #[test]
    fn test_build_leave_comment_request() {
        let root_id = AccountId::new_unchecked("kudos.near".to_owned());
        let sender_id = AccountId::new_unchecked("test1.near".to_owned());
        let receiver_id = AccountId::new_unchecked("test2.near".to_owned());
        let mut unique_id = IncrementalUniqueId::default();
        let kudos_id = KudosId::from(unique_id.inc());
        let comment_id = CommentId::from(unique_id.inc());

        let json_text = serde_json::to_string(
            &super::build_leave_comment_request(
                &root_id,
                &receiver_id,
                &kudos_id,
                &comment_id,
                &EncodedCommentary::try_from(&Commentary {
                    sender_id: &sender_id,
                    message: &Value::String("some commentary text".to_string()),
                    timestamp: U64(1234567890),
                    parent_comment_id: None,
                })
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            json_text,
            r#"{"kudos.near":{"kudos":{"test2.near":{"1":{"comments":{"2":"eyJtIjoic29tZSBjb21tZW50YXJ5IHRleHQiLCJzIjoidGVzdDEubmVhciIsInQiOiIxMjM0NTY3ODkwIn0="}}}}}}"#
        );
    }

    #[test]
    fn test_build_get_kudos_by_id_request() {
        let root_id = AccountId::new_unchecked("kudos.near".to_owned());
        let receiver_id = AccountId::new_unchecked("test2.near".to_owned());
        let next_kudos_id = KudosId::from(IncrementalUniqueId::default().next());
        assert_eq!(
            &super::build_get_kudos_by_id_request(&root_id, &receiver_id, &next_kudos_id),
            "kudos.near/kudos/test2.near/1/*"
        );
    }

    #[test]
    fn test_build_get_kudos_comment_by_id_request() {
        let root_id = AccountId::new_unchecked("kudos.near".to_owned());
        let receiver_id = AccountId::new_unchecked("test2.near".to_owned());
        let mut id = IncrementalUniqueId::default();
        let next_kudos_id = KudosId::from(id.inc());
        let next_comment_id = CommentId::from(id.inc());
        assert_eq!(
            &super::build_get_kudos_comment_by_id_request(
                &root_id,
                &receiver_id,
                &next_kudos_id,
                &next_comment_id
            ),
            "kudos.near/kudos/test2.near/1/comments/2"
        );
    }

    #[test]
    fn test_extract_kudos_id_sender_from_response() {
        // valid kudos response
        assert_eq!(
            super::extract_kudos_id_sender_from_response(
                "test.near/kudos/user1.near/1/*",
                &mut json!({
                    "test.near": {
                      "kudos": {
                        "user1.near": {
                          "1": {
                            "sender_id": "user2.near"
                          }
                        }
                      }
                    }
                })
            ),
            Some(AccountId::new_unchecked("user2.near".to_owned()))
        );
        // invalid kudos response
        assert!(super::extract_kudos_id_sender_from_response(
            "test.near/kudos/user1.near/1/*",
            &mut json!({
                "test.near": {
                  "kudos": {
                    "user1.near": {
                      "1": {}
                    }
                  }
                }
            })
        )
        .is_none());
        // different kudos root id
        assert!(super::extract_kudos_id_sender_from_response(
            "test.near/kudos/user1.near/1/*",
            &mut json!({
                "test1.near": {
                  "kudos": {
                    "user1.near": {
                      "1": {
                        "sender_id": "user2.near"
                      }
                    }
                  }
                }
            })
        )
        .is_none());
        // no response
        assert!(super::extract_kudos_id_sender_from_response(
            "test.near/kudos/user1.near/1/*",
            &mut json!({})
        )
        .is_none());
    }

    #[test]
    fn test_extract_kudos_encoded_comment_by_id_from_response() {
        // valid kudos base64-encoded comment response
        assert_eq!(
            super::extract_kudos_encoded_comment_by_id_from_response(
                "test.near/kudos/user1.near/1/*",
                &CommentId::new_unchecked(2),
                &mut json!({
                    "test.near": {
                      "kudos": {
                        "user1.near": {
                          "1": {
                            "comments": {
                              "2": "eyJtIjoiY29tbWVudGFyeSB0ZXN0IiwicCI6IjEiLCJzIjoidXNlci5uZWFyIiwidCI6IjEyMzQ1Njc4OTAifQ"
                            }
                          }
                        }
                      }
                    }
                })
            ),
            Some(EncodedCommentary::new_unchecked("eyJtIjoiY29tbWVudGFyeSB0ZXN0IiwicCI6IjEiLCJzIjoidXNlci5uZWFyIiwidCI6IjEyMzQ1Njc4OTAifQ".to_owned()))
        );
        // invalid kudos base64-encoded comment response
        assert!(super::extract_kudos_encoded_comment_by_id_from_response(
            "test.near/kudos/user1.near/1/*",
            &CommentId::new_unchecked(2),
            &mut json!({
                "test.near": {
                  "kudos": {
                    "user1.near": {
                      "1": {
                        "comments": {}
                      }
                    }
                  }
                }
            })
        )
        .is_none());
        // different parent commentary id
        assert!(super::extract_kudos_encoded_comment_by_id_from_response(
            "test.near/kudos/user1.near/1/*",
            &CommentId::new_unchecked(3),
            &mut json!({
                "test.near": {
                  "kudos": {
                    "user1.near": {
                      "1": {
                        "comments": {
                          "2": "eyJtIjoiY29tbWVudGFyeSB0ZXN0IiwicCI6IjEiLCJzIjoidXNlci5uZWFyIiwidCI6IjEyMzQ1Njc4OTAifQ"
                        }
                      }
                    }
                  }
                }
            })
        )
        .is_none());
    }

    #[test]
    fn test_remove_key_from_json() {
        let mut json = json!({
            "abc": "test",
            "remove_me": "test2",
            "test": {
                "remove_me": "test3",
                "test1": {
                    "remove_me": "test4"
                }
            }
        });

        // key not exist or nothing to remove
        assert!(remove_key_from_json(&mut json, "").is_none());
        assert!(remove_key_from_json(&mut json, "testtest").is_none());
        assert!(remove_key_from_json(&mut json, "test_abc/test_def").is_none());
        assert_eq!(
            json.to_string(),
            r#"{"abc":"test","remove_me":"test2","test":{"remove_me":"test3","test1":{"remove_me":"test4"}}}"#
        );
        // remove key from root
        assert_eq!(
            remove_key_from_json(&mut json, "remove_me"),
            Some(json!("test2"))
        );
        assert_eq!(
            json.to_string(),
            r#"{"abc":"test","test":{"remove_me":"test3","test1":{"remove_me":"test4"}}}"#
        );
        // remove nested key
        assert_eq!(
            remove_key_from_json(&mut json, "test/remove_me"),
            Some(json!("test3"))
        );
        assert_eq!(
            json.to_string(),
            r#"{"abc":"test","test":{"test1":{"remove_me":"test4"}}}"#
        );
        // remove deeply nested key
        assert_eq!(
            remove_key_from_json(&mut json, "test/test1/remove_me"),
            Some(json!("test4"))
        );
        assert_eq!(json.to_string(), r#"{"abc":"test","test":{"test1":{}}}"#);
    }

    #[test]
    fn test_display_deposit_requirement_in_near() {
        assert_eq!(
            display_deposit_requirement_in_near(parse_near!("0.0005 NEAR")).as_str(),
            "Requires exact amount of attached deposit 0.0005 NEAR"
        );
        assert_eq!(
            display_deposit_requirement_in_near(parse_near!("0.00051 NEAR")).as_str(),
            "Requires exact amount of attached deposit 0.00051 NEAR"
        );
        assert_eq!(
            display_deposit_requirement_in_near(parse_near!("0.000553 NEAR")).as_str(),
            "Requires exact amount of attached deposit 0.00055 NEAR"
        );
    }
}
