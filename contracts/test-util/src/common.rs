use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::Serialize;
use near_sdk::AccountId;
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

#[derive(BorshSerialize, BorshDeserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq))]
pub struct Claim {
    pub claimer: AccountId,
    /// external, Ethereum compatible address. Must be a hex string, can start with "0x".
    pub external_id: String,
    /// unix time (seconds) when the claim was signed
    pub timestamp: u64,
    /// indicates whether the user has passed a KYC or not
    pub verified_kyc: bool,
}
