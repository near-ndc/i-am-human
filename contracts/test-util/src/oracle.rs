use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::Serialize;
use uuid::Uuid;

/// External account id represented as hexadecimal string
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct ExternalAccountId(String);

impl std::fmt::Display for ExternalAccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for ExternalAccountId {
    fn from(value: Uuid) -> Self {
        let mut buf = [0u8; uuid::fmt::Simple::LENGTH];
        Self(value.as_simple().encode_lower(&mut buf).to_owned())
    }
}

impl ExternalAccountId {
    pub fn gen() -> Self {
        Uuid::new_v4().into()
    }
}

#[derive(Debug, Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct SignedClaim {
    pub claim_b64: String,
    pub claim_sig: String,
}
