use ed25519_dalek::Signer;
use near_crypto::{PublicKey, SecretKey};
use near_sdk::{test_utils::VMContextBuilder, AccountId, Balance, Gas};

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
