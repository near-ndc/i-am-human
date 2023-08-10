use std::str::Chars;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{base64, env, AccountId};
use uint::hex;

pub use crate::errors::*;

pub const PUBLIC_KEY_LENGTH: usize = 32;

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

mod sys {
    extern "C" {
        #[allow(dead_code)]
        pub fn ed25519_verify(
            sig_len: u64,
            sig_ptr: u64,
            msg_len: u64,
            msg_ptr: u64,
            pub_key_len: u64,
            pub_key_ptr: u64,
        ) -> u64;
    }
}

#[cfg(not(all(test, not(target_arch = "wasm32"))))]
pub fn ed25519_verify(signature: &[u8; 64], message: &[u8], public_key: &[u8; 32]) -> bool {
    unsafe {
        sys::ed25519_verify(
            signature.len() as _,
            signature.as_ptr() as _,
            message.len() as _,
            message.as_ptr() as _,
            public_key.len() as _,
            public_key.as_ptr() as _,
        ) == 1
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
pub fn ed25519_verify(signature: &[u8; 64], message: &[u8], public_key: &[u8; 32]) -> bool {
    return true;
}

pub fn verify_claim(
    pubkey: &[u8; PUBLIC_KEY_LENGTH],
    claim: Vec<u8>,
    claim_sig: &[u8; 64],
) -> Result<(), CtrError> {
    let valid = ed25519_verify(claim_sig, &claim, pubkey);
    if !valid {
        return Err(CtrError::Signature("invalid signature".to_string()));
    } else {
        Ok(())
    }
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
    // check if implicit account only for mainnet and testnet
    if num_dots == 0 {
        let a = env::current_account_id();
        let a = a.as_str();
        if a.ends_with(".near") || a.ends_with(".testnet") {
            return len == 64 && all_hex;
        }
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

    #[test]
    fn check_pub_key_len() {
        assert_eq!(ed25519_dalek::PUBLIC_KEY_LENGTH, PUBLIC_KEY_LENGTH);
    }

    #[test]
    fn pubkey_near_crypto() {
        //let sk = near_crypto::SecretKey::from_str("ed25519:...").unwrap();
        let sk = near_crypto::SecretKey::from_random(near_crypto::KeyType::ED25519);
        let k = match sk.clone() {
            near_crypto::SecretKey::ED25519(k) => ed25519_dalek::Keypair::from_bytes(&k.0).unwrap(),
            _ => panic!("expecting ed25519 key"),
        };

        let pk_bs58 = near_sdk::bs58::encode(k.public).into_string();
        let pk_b64 = near_sdk::base64::encode(k.public.as_bytes().to_vec());
        let sk_str = near_sdk::bs58::encode(k.secret).into_string();
        let sk_str2 = sk.to_string();
        println!(
            "pubkey_bs58={}  pubkey_b64={}\nsecret={} {}",
            pk_bs58, pk_b64, sk_str, sk_str2,
        );

        // let sk2 = near_crypto::SecretKey::from_str(
        //     "secp256k1:AxynSCWRr2RrBXbzcbykYTo5vPmCkMf35s1D1bXV8P51",
        // )
        // .unwrap();
        // println!("\nsecp: {}, public: {}", sk2, sk2.public_key());

        // assert!(false);
    }
}
