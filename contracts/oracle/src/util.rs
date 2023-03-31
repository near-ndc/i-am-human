use std::num::ParseIntError;

use ed25519_dalek::PUBLIC_KEY_LENGTH;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{base64, AccountId};

pub use crate::errors::*;

type CtrResult<T> = Result<T, CtrError>;

#[derive(BorshSerialize, BorshDeserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq))]
pub struct Claim {
    pub claimer: AccountId,
    /// external, Ethereum compatible address. Must be a hex string, can start with "0x".
    pub external_id: String,
    /// unix time (seconds) when the claim was signed
    pub timestamp: u64,
}

pub(crate) fn normalize_external_id(id: String) -> Result<Vec<u8>, CtrError> {
    let id = id.strip_prefix("0x").unwrap_or(&id).to_lowercase();
    hex_decode(&id).map_err(|s| CtrError::BadRequest(format!("claim.external_id: {}", s)))
}

pub(crate) fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("invalid length".to_owned());
    }
    let r: Result<Vec<u8>, ParseIntError> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect();
    r.map_err(|err| err.to_string())
}

pub fn b64_decode(arg: &str, data: String) -> CtrResult<Vec<u8>> {
    return base64::decode(data).map_err(|e| CtrError::B64Err {
        arg: arg.to_string(),
        err: e,
    });
}

pub fn pubkey_from_b64(pubkey: String) -> [u8; PUBLIC_KEY_LENGTH] {
    let pk_bz = base64::decode(pubkey).expect("authority_pubkey is not a valid standard base64");
    pk_bz.try_into().expect("authority pubkey must be 32 bytes")
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn check_hex(s: &str, r: Vec<u8>) -> Result<(), String> {
        let b = hex_decode(s)?;
        assert_eq!(b.len(), r.len());
        assert_eq!(b, r);
        Ok(())
    }

    #[test]
    fn test_hex_decode() {
        check_hex("08", vec![8]).unwrap();
        check_hex("10", vec![16]).unwrap();
        check_hex("aa", vec![170]).unwrap();
        check_hex("1203", vec![18, 3]).unwrap();
        check_hex("1223", vec![18, 35]).unwrap();

        let h = "b4bf0f23c702efb8a9da87a94095e28de3d21cc3";
        let b = hex_decode(h).unwrap();
        assert_eq!(b.len(), 20);
        assert_eq!(b[0], 11 * 16 + 4);

        assert!(hex_decode("8").unwrap_err().contains("invalid len"));
        assert!(hex_decode("123").unwrap_err().contains("invalid len"));
        assert_eq!(
            hex_decode("0x").unwrap_err(),
            "invalid digit found in string"
        );
        assert_eq!(
            hex_decode("xx").unwrap_err(),
            "invalid digit found in string"
        );
        assert_eq!(
            hex_decode("1w").unwrap_err(),
            "invalid digit found in string"
        );
    }
}
