use cid::Cid;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U64;
use near_sdk::serde::{self, de, Deserialize, Deserializer, Serialize, Serializer};
use near_sdk::serde_json::Value;
use near_sdk::{serde_json, AccountId, BorshStorageKey};
use std::fmt::Display;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

/// This type represents a unique incremental identifier
#[derive(BorshDeserialize, BorshSerialize)]
pub struct IncrementalUniqueId(U64);

impl IncrementalUniqueId {
    /// Return [`u64`] representation of this [`IncrementalUniqueId`]
    pub fn as_u64(&self) -> u64 {
        self.0 .0
    }

    /// Increment self-stored value and returns self-reference
    pub fn inc(&mut self) -> &Self {
        self.0 = self.next().0;
        self
    }

    /// Compute the next identifier
    pub fn next(&self) -> Self {
        Self((self.as_u64() + 1).into())
    }
}

impl Default for IncrementalUniqueId {
    fn default() -> Self {
        Self(0.into())
    }
}

/// This type represents a unique identifier of the kudos.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
#[serde(crate = "near_sdk::serde")]
pub struct KudosId(U64);

impl From<IncrementalUniqueId> for KudosId {
    fn from(value: IncrementalUniqueId) -> Self {
        Self(value.0)
    }
}

impl From<&IncrementalUniqueId> for KudosId {
    fn from(value: &IncrementalUniqueId) -> Self {
        Self(value.0)
    }
}

impl Display for KudosId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0 .0, f)
    }
}

/// This type represents a unique identifier of the commentary message.
#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, Eq, PartialEq))]
#[serde(crate = "near_sdk::serde")]
pub struct CommentId(U64);

impl CommentId {
    /// Creates [`CommentId`] from identifier without guarantee for validness & uniqueness
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_unchecked(id: u64) -> Self {
        Self(id.into())
    }
}

impl Hash for CommentId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0 .0.hash(state);
    }
}

impl From<IncrementalUniqueId> for CommentId {
    fn from(value: IncrementalUniqueId) -> Self {
        Self(value.0)
    }
}

impl From<&IncrementalUniqueId> for CommentId {
    fn from(value: &IncrementalUniqueId) -> Self {
        Self(value.0)
    }
}

impl Display for CommentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0 .0, f)
    }
}

/// The type of storage key used as key prefix in contract storage
#[derive(BorshStorageKey, BorshSerialize)]
pub(crate) enum StorageKey {
    Kudos,
}

/// Commentary message data struct which serializes to base64-encoded [`String`] for subsequent store in NEAR social db
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct Commentary<'a> {
    /// A message with escaped characters to guarantee safety of stringification
    pub message: &'a Value,
    /// A valid [`AccountId`] of a message sender
    pub sender_id: &'a AccountId,
    /// The timestamp in milliseconds when commentary message were prepared
    pub timestamp: U64,
    /// Parent commentary id which were replied
    pub parent_comment_id: Option<&'a CommentId>,
}

/// Raw commentary message data struct which serializes to [`Value`](near_sdk::serde_json::Value)
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct CommentaryRaw<'a> {
    /// A message with escaped characters to guarantee safety of stringification
    #[serde(rename = "m")]
    pub message: &'a Value,
    /// A valid [`AccountId`] of a message sender
    #[serde(rename = "s")]
    pub sender_id: &'a AccountId,
    /// The timestamp in milliseconds when commentary message were prepared
    #[serde(rename = "t")]
    pub timestamp: U64,
    /// Parent commentary id which were replied
    #[serde(rename = "p", skip_serializing_if = "Option::is_none")]
    pub parent_comment_id: Option<&'a CommentId>,
}

impl Serialize for Commentary<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: near_sdk::serde::Serializer,
    {
        let encoded = near_sdk::base64::encode(
            serde_json::to_string(&CommentaryRaw {
                message: self.message,
                sender_id: self.sender_id,
                timestamp: self.timestamp,
                parent_comment_id: self.parent_comment_id,
            })
            .map_err(near_sdk::serde::ser::Error::custom)?,
        );

        serializer.serialize_str(&encoded)
    }
}

/// This type represents a [`String`] for which only ascii alphanumeric characters, underscores and gyphens are allowed to use
#[derive(Deserialize, Serialize, Ord, PartialOrd, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Clone, Debug))]
#[serde(crate = "near_sdk::serde")]
pub struct Hashtag(String);

impl Hashtag {
    /// Create [`Hashtag`] from ascii ref string, verify maximum length and check for allowed characters
    pub fn new(hashtag: &str, max_lenth: usize) -> Result<Self, &'static str> {
        if hashtag.len() > max_lenth {
            return Err("Hashtag max text length exceeded");
        }

        if hashtag.contains(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, '_' | '-')) {
            return Err(
                "Only alphanumeric characters, underscores and gyphens are allowed for hashtag",
            );
        }

        Ok(Self(hashtag.to_owned()))
    }

    /// Creates [`Hashtag`] from ref string without length and characters check
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_unchecked(hashtag: &str) -> Self {
        Self(hashtag.to_owned())
    }
}

/// This type represents a JSON [`String`] view of [`Commentary`]
#[derive(Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Clone, Debug, PartialEq))]
#[serde(crate = "near_sdk::serde")]
pub struct EncodedCommentary(String);

impl EncodedCommentary {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Creates [`EncodedCommentary`] from [`String`] without verification if it can be deserialized
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_unchecked(encoded: String) -> Self {
        Self(encoded)
    }
}

impl TryFrom<&Commentary<'_>> for EncodedCommentary {
    type Error = String;

    fn try_from(value: &Commentary<'_>) -> Result<Self, Self::Error> {
        serde_json::to_value(value)
            .and_then(|val| {
                val.as_str()
                    .map(|s| Self(s.to_owned()))
                    .ok_or(serde::ser::Error::custom("Not a string"))
            })
            .map_err(|e| format!("Unable to encode commentary: {e}"))
    }
}

impl Display for EncodedCommentary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

/// This type represents a wrapped serializable version of [`Cid`]
pub struct WrappedCid(Cid);

impl WrappedCid {
    /// Creates [`WrappedCid`] from ref string
    pub fn new(cid: &str) -> Result<Self, &'static str> {
        if cid.len() > 64 {
            return Err("Cid is too long");
        }
        Cid::from_str(cid)
            .map_err(|_| "Not a valid Cid")
            .map(WrappedCid)
    }
}

impl Display for WrappedCid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<'de> Deserialize<'de> for WrappedCid {
    fn deserialize<D>(deserializer: D) -> Result<WrappedCid, D::Error>
    where
        D: Deserializer<'de>,
    {
        let cid_text = <String as Deserialize>::deserialize(deserializer)?;

        Cid::from_str(&cid_text)
            .map(WrappedCid)
            .map_err(|e| de::Error::custom(format!("Failed to deserialize CID: {e:?}")))
    }
}

impl Serialize for WrappedCid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

/// The type of a kudos given
///
/// [`Kudos`](KudosKind::Kudos) represents a positive kudos, while [`Ding`](KudosKind::Ding) represents a negative one
#[derive(Serialize, Deserialize, Default, PartialEq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Clone, Debug))]
#[serde(crate = "near_sdk::serde")]
pub enum KudosKind {
    #[default]
    #[serde(rename = "k")]
    Kudos,
    #[serde(rename = "d")]
    Ding,
}

impl Display for KudosKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            Self::Kudos => "k",
            Self::Ding => "d",
        };

        write!(f, "{kind}")
    }
}

#[cfg(test)]
mod tests {
    use crate::{CommentId, Commentary, EncodedCommentary, Hashtag, WrappedCid};
    use near_sdk::json_types::U64;
    use near_sdk::AccountId;
    use near_sdk::serde_json::Value;

    #[test]
    fn test_commentary_encoding() {
        let comment = EncodedCommentary::try_from(&Commentary {
            sender_id: &AccountId::new_unchecked("user.near".to_owned()),
            message: &Value::String("commentary test".to_string()),
            timestamp: U64(1234567890),
            parent_comment_id: None,
        })
        .unwrap();
        assert_eq!(
            comment.as_str(),
            "eyJtIjoiY29tbWVudGFyeSB0ZXN0IiwicyI6InVzZXIubmVhciIsInQiOiIxMjM0NTY3ODkwIn0="
        );

        let comment = EncodedCommentary::try_from(&Commentary {
            sender_id: &AccountId::new_unchecked("user.near".to_owned()),
            message: &Value::String("commentary test".to_string()),
            timestamp: U64(1234567890),
            parent_comment_id: Some(&CommentId::new_unchecked(1u64)),
        })
        .unwrap();
        assert_eq!(
            comment.as_str(),
            "eyJtIjoiY29tbWVudGFyeSB0ZXN0IiwicyI6InVzZXIubmVhciIsInQiOiIxMjM0NTY3ODkwIiwicCI6IjEifQ=="
        );
    }

    #[test]
    fn test_hashtag_from_str() {
        assert!(Hashtag::new("validhashtag", 32).is_ok());
        assert!(Hashtag::new("val1dhAshta9", 32).is_ok());
        assert!(Hashtag::new("va-li-d_hashtag", 32).is_ok());
        assert!(Hashtag::new("invalid+hashtag", 32).is_err());
        assert!(Hashtag::new("invalidha$ht@g", 32).is_err());
        assert!(Hashtag::new("toolonghashtag", 8).is_err());
    }

    #[test]
    fn test_wrapped_cid() {
        assert!(WrappedCid::new("invalid_cid").is_err());
        // Verify V1 CID
        assert_eq!(
            WrappedCid::new("bafkreieq5jui4j25lacwomsqgjeswwl3y5zcdrresptwgmfylxo2depppq")
                .unwrap()
                .to_string()
                .as_str(),
            "bafkreieq5jui4j25lacwomsqgjeswwl3y5zcdrresptwgmfylxo2depppq"
        );
        // Verify V0 CID
        assert_eq!(
            &format!(
                "{}",
                WrappedCid::new("QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n").unwrap()
            ),
            "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n"
        );
    }
}
