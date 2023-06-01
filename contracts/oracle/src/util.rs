use std::str::Chars;

use ed25519_dalek::PUBLIC_KEY_LENGTH;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{base64, AccountId};
use uint::hex;

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
    /// indicates whether the user has passed a KYC or not
    pub verified_kyc: bool,
}

pub(crate) fn normalize_external_id(id: String) -> Result<Vec<u8>, CtrError> {
    let id = id.strip_prefix("0x").unwrap_or(&id).to_lowercase();
    hex::decode(id).map_err(|s| CtrError::BadRequest(format!("claim.external_id: {}", s)))
}

pub fn b64_decode(arg: &str, data: String) -> CtrResult<Vec<u8>> {
    base64::decode(data).map_err(|e| CtrError::B64Err {
        arg: arg.to_string(),
        err: e,
    })
}

pub fn pubkey_from_b64(pubkey: String) -> [u8; PUBLIC_KEY_LENGTH] {
    let pk_bz = base64::decode(pubkey).expect("authority_pubkey is not a valid standard base64");
    pk_bz.try_into().expect("authority pubkey must be 32 bytes")
}

/// only root accounts and implicit accounts are supported
pub(crate) fn is_supported_account(account: Chars) -> bool {
    let mut num_dots = 0;
    let mut len = 0;
    let mut all_hex = true;
    for c in account {
        len += 1;
        if c == '.' {
            num_dots += 1;
        }
        all_hex = all_hex && c.is_ascii_hexdigit();
    }
    if num_dots == 1 {
        return true;
    }
    // check if implicit account
    if num_dots == 0 && len == 64 && all_hex {
        return true;
    }
    false
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use uint::hex::FromHexError;

    use super::*;

    fn check_hex(s: &str, r: Vec<u8>) -> Result<(), FromHexError> {
        let b = hex::decode(s)?;
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
        let b = hex::decode(h).unwrap();
        assert_eq!(b.len(), 20);
        assert_eq!(b[0], 11 * 16 + 4);

        assert_eq!(hex::decode("8").unwrap_err(), FromHexError::OddLength);
        assert_eq!(hex::decode("123").unwrap_err(), FromHexError::OddLength);
        assert_eq!(
            hex::decode("0x").unwrap_err(),
            FromHexError::InvalidHexCharacter { c: 'x', index: 1 }
        );
        assert_eq!(
            hex::decode("xx").unwrap_err(),
            FromHexError::InvalidHexCharacter { c: 'x', index: 0 },
        );
        assert_eq!(
            hex::decode("1w").unwrap_err(),
            FromHexError::InvalidHexCharacter { c: 'w', index: 1 },
        );
    }
}
