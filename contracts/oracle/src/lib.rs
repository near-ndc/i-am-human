use base64::{engine::general_purpose::STANDARD as base64, Engine as _};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
// use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{env, near_bindgen, require, AccountId, Balance, PanicOnDefault};

/// Balance of one mili NEAR, which is 10^21 Yocto NEAR.
pub const MILI_NEAR: Balance = 1_000_000_000_000_000_000_000;
pub const SECOND: u64 = 1_000_000_000;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// ed25519 pub key (could be same as a NEAR pub key)
    pub authority_pubkey: Vec<u8>,
    /// max duration (in seconds) a claim is valid for processing
    pub claim_ttl: u64,
    // TODO (will be copied from community-sbt after updates):
    // metadata
    // balances
    // token_data
    // sbt_data
}

#[near_bindgen]
impl Contract {
    /// @authority_pubkey: base64 of authority pub key.
    /// @claim_ttl: max duration (in seconds) a claim is valid for processing.
    ///   If zero default (14 days) is used.
    #[init]
    pub fn new(authority_pubkey: String, claim_ttl: u64) -> Self {
        let claim_ttl = if claim_ttl == 0 {
            60 * 60 * 24 * 14 // 2 weeks
        } else {
            claim_ttl
        };
        let authority_pubkey = base64
            .decode(authority_pubkey)
            .expect("authority_pubkey is not a valid standard base64");
        Self {
            claim_ttl,
            authority_pubkey,
        }
    }

    // /// returns information about specific token ID
    // pub fn sbt(&self, token_id: u64) -> Option<Token> {
    //     None()
    // }

    /**************
     * TRANSACTIONS
     **************/

    /// Mints a new SBT for the transaction signer.
    /// @claim_b64: standard base64 borsh serialized Claim (same bytes as used for the claim signature)
    /// If `metadata.expires_at` is None then we set it to ` now+self.ttl`.
    /// Panics if `metadata.expires_at > now+self.ttl`.
    pub fn sbt_mint(&mut self, claim_b64: String, claim_sig: String) {
        let _sig = base64
            .decode(claim_sig)
            .expect("claim_sig is not a valid standard base64");
        // TODO: check signature

        let claim_bytes = base64
            .decode(claim_b64)
            .expect("claim_b64 is not a valid standard base64");
        // TODO: check if we can handle the double reference in a better way
        let claim = Claim::deserialize(&mut &claim_bytes[..])
            .expect("claim is not borsh -> base64 serialized data");
        let now = env::block_timestamp() / SECOND;
        require!(
            claim.timestamp <= now && now - self.claim_ttl < claim.timestamp,
            "claim expired"
        );
        require!(
            claim.claimer == env::signer_account_id(),
            "claimer is not the transaction signer"
        );

        // TODO: check if claimer and external_id are not yet registered and issue SBT
    }
}

pub type TokenId = u64;

#[derive(BorshSerialize, BorshDeserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq))]
pub struct Claim {
    claimer: AccountId,
    external_id: String,
    /// unix time (seconds) when the claim was signed
    timestamp: u64,
}
