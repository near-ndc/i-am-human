use std::str::Chars;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{base64, env, AccountId};
use uint::hex;

pub use crate::errors::*;

pub const PUBLIC_KEY_LEN: usize = 32;
pub const SIGNATURE_LEN: usize = 64;

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

pub fn pubkey_from_b64(pubkey: String) -> [u8; PUBLIC_KEY_LEN] {
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
pub fn ed25519_verify(signature: &[u8; 64], message: &[u8], pubkey: &[u8; 32]) -> bool {
    unsafe {
        sys::ed25519_verify(
            signature.len() as _,
            signature.as_ptr() as _,
            message.len() as _,
            message.as_ptr() as _,
            pubkey.len() as _,
            pubkey.as_ptr() as _,
        ) == 1
    }
}

#[cfg(test)]
use ed25519_dalek::{PublicKey, Signature, Verifier};

#[cfg(all(test, not(target_arch = "wasm32")))]
pub fn ed25519_verify(signature: &[u8; 64], message: &[u8], pubkey: &[u8; 32]) -> bool {
    let pk = PublicKey::from_bytes(pubkey).unwrap();
    match Signature::from_bytes(signature) {
        Ok(sig) => pk.verify(message, &sig).is_ok(),
        Err(_) => false,
    }
}

pub fn verify_claim(
    claim_sig: &Vec<u8>,
    claim: &Vec<u8>,
    pubkey: &[u8; PUBLIC_KEY_LEN],
) -> Result<(), CtrError> {
    let claim_sig: &[u8; SIGNATURE_LEN] = claim_sig
        .as_slice()
        .try_into()
        .expect("signature must be 64 bytes");
    match ed25519_verify(claim_sig, claim, pubkey) {
        true => Ok(()),
        false => Err(CtrError::Signature("invalid signature".to_string())),
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
pub mod tests {
    extern crate ed25519_dalek;
    extern crate rand;
    use ed25519_dalek::{Keypair, Signer};
    use rand::rngs::OsRng;

    use uint::hex::FromHexError;

    use super::*;
    use crate::checks::tests::deserialize_claim;

    pub fn gen_key() -> Keypair {
        let mut csprng = OsRng {};
        Keypair::generate(&mut csprng)
    }

    pub fn acc_claimer() -> AccountId {
        "user1.near".parse().unwrap()
    }

    pub fn b64_encode(data: Vec<u8>) -> String {
        near_sdk::base64::encode(data)
    }

    /// @timestamp: in seconds
    pub fn mk_claim(timestamp: u64, external_id: &str, is_verified_kyc: bool) -> Claim {
        Claim {
            claimer: acc_claimer(),
            external_id: external_id.to_string(),
            timestamp,
            verified_kyc: is_verified_kyc,
        }
    }

    // returns b64 serialized claim and signature
    pub fn sign_claim(c: &Claim, k: &Keypair) -> (String, String) {
        let c_bz = c.try_to_vec().unwrap();
        let sig = k.sign(&c_bz);
        let sig_bz = sig.to_bytes();
        (b64_encode(c_bz), b64_encode(sig_bz.to_vec()))
    }

    pub fn mk_claim_sign(
        timestamp: u64,
        external_id: &str,
        k: &Keypair,
        is_verified_kyc: bool,
    ) -> (Claim, String, String) {
        let c = mk_claim(timestamp, external_id, is_verified_kyc);
        let (c_str, sig) = sign_claim(&c, k);
        (c, c_str, sig)
    }

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
    fn claim_serialization() {
        let c = mk_claim(1677621259142, "some_111#$!", false);
        let claim_bz = c.try_to_vec().unwrap();
        let claim_str = b64_encode(claim_bz);
        let claim2 = deserialize_claim(&claim_str);
        assert_eq!(c, claim2, "serialization should work");
    }

    #[test]
    fn check_pub_key_len() {
        assert_eq!(ed25519_dalek::PUBLIC_KEY_LENGTH, PUBLIC_KEY_LEN);
        assert_eq!(ed25519_dalek::SIGNATURE_LENGTH, SIGNATURE_LEN);
    }

    #[test]
    fn test_verify_claim() {
        let k = gen_key();
        let (_, c_str, sig) = mk_claim_sign(10000, "0x12", &k, false);
        let claim_bytes = b64_decode("claim_b64", c_str).unwrap();
        let signature = b64_decode("sign_b64", sig).unwrap();
        let res = verify_claim(&signature, &claim_bytes, &k.public.to_bytes());
        assert!(res.is_ok(), "verification result: {:?}", res);

        let pk2 = gen_key().public;
        // let pk_bs58 = near_sdk::bs58::encode(k.public).into_string();
        // println!(">>> pub {:?}", b64_encode(pk2.as_bytes().to_vec()));
        let res = verify_claim(&signature, &claim_bytes, pk2.as_bytes());
        assert!(res.is_err(), "verification result: {:?}", res);

        let pk3_bytes = pubkey_from_b64("FGoAI6DXghOSK2ZaKVT/5lSP4X4JkoQQphv1FD4YRto=".to_string());
        assert_ne!(pk3_bytes[0], 0);
        let res = verify_claim(&signature, &claim_bytes, &pk3_bytes);
        assert!(res.is_err(), "verification result: {:?}", res);
    }
}
