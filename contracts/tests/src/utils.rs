use crate::common::{ExternalAccountId, SignedClaim};
use chrono::Utc;
use ed25519_dalek::Signer;
use near_crypto::{PublicKey, SecretKey, Signature};
use near_sdk::{borsh::BorshSerialize, test_utils::VMContextBuilder, AccountId, Balance, Gas};
use oracle_sbt::util::Claim;

pub const MAX_GAS: Gas = Gas(300_000_000_000_000);

pub fn build_default_context(
    predecessor_account_id: AccountId,
    deposit: Option<Balance>,
    prepaid_gas: Option<Gas>,
) -> VMContextBuilder {
    let mut builder = VMContextBuilder::new();
    builder
        .signer_account_id(predecessor_account_id.clone())
        .predecessor_account_id(predecessor_account_id)
        .prepaid_gas(prepaid_gas.unwrap_or(MAX_GAS))
        .attached_deposit(deposit.unwrap_or_default());
    builder
}

pub fn build_signed_claim(
    claimer: AccountId,
    external_id: ExternalAccountId,
    verified_kyc: bool,
    sec_key: &SecretKey,
) -> anyhow::Result<SignedClaim> {
    let claim_raw = Claim {
        claimer,
        external_id: external_id.to_string(),
        verified_kyc,
        timestamp: Utc::now().timestamp() as u64,
    }
    .try_to_vec()?;

    let sign = sign_bytes(&claim_raw, sec_key);

    assert!(
        Signature::ED25519(ed25519_dalek::Signature::from_bytes(&sign)?)
            .verify(&claim_raw, &sec_key.public_key())
    );

    Ok(SignedClaim {
        claim_b64: near_sdk::base64::encode(claim_raw),
        claim_sig: near_sdk::base64::encode(sign),
    })
}

pub fn generate_keys() -> (SecretKey, PublicKey) {
    let seckey = SecretKey::from_random(near_crypto::KeyType::ED25519);
    let pubkey = seckey.public_key();

    (seckey, pubkey)
}

pub fn sign_bytes(bytes: &[u8], sec_key: &SecretKey) -> Vec<u8> {
    match sec_key {
        SecretKey::ED25519(secret_key) => {
            let keypair = ed25519_dalek::Keypair::from_bytes(&secret_key.0).unwrap();
            keypair.sign(bytes).to_bytes().to_vec()
        }
        _ => unimplemented!(),
    }
}
