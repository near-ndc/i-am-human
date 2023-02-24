use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, LookupMap, UnorderedSet};
use near_sdk::json_types::U64;
use near_sdk::{base64, env, near_bindgen, require, AccountId, PanicOnDefault};

use sbt::*;

// TODO
// use near_sdk::bs58 -- use public key in the base58 format

pub use crate::errors::*;
pub use crate::interfaces::*;
pub use crate::storage::*;

mod errors;
mod interfaces;
mod storage;

/// 1s in nano seconds.
pub const SECOND: u64 = 1_000_000_000;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// registry of burned accounts.
    pub registry: AccountId,

    pub balances: LookupMap<AccountId, TokenId>,
    pub token_data: LookupMap<TokenId, TokenData>,
    // contract metadata
    pub metadata: LazyOption<SBTContractMetadata>,

    pub next_token_id: TokenId,
    /// max duration (in seconds) a claim is valid for processing
    pub claim_ttl: u64,
    /// SBT ttl until expire in miliseconds (expire=issue_time+sbt_ttl)
    pub sbt_ttl_ms: u64,
    /// ed25519 pub key (could be same as a NEAR pub key)
    pub authority_pubkey: Vec<u8>,
    pub used_identities: UnorderedSet<String>,

    // TODO: remove, to test purposes only
    pub admin: AccountId,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    /// @authority_pubkey: base64 of authority pub key.
    /// @metadata: NFT like metadata about the contract.
    /// @registry: the SBT registry responsable for the "soul transfer".
    /// @claim_ttl: max duration (in seconds) a claim is valid for processing.
    ///   If zero default (1 day) is used.
    #[init]
    pub fn new(
        authority_pubkey: String,
        metadata: SBTContractMetadata,
        registry: AccountId,
        claim_ttl: u64,
        admin: AccountId,
    ) -> Self {
        let claim_ttl = if claim_ttl == 0 {
            3600 * 24 // 1 day
        } else {
            claim_ttl
        };
        let authority_pubkey = base64::decode(authority_pubkey)
            .expect("authority_pubkey is not a valid standard base64");

        Self {
            registry,

            balances: LookupMap::new(StorageKey::Balances),
            token_data: LookupMap::new(StorageKey::TokenData),
            metadata: LazyOption::new(StorageKey::ContractMetadata, Some(&metadata)),
            next_token_id: 1,
            claim_ttl,
            sbt_ttl_ms: 1000 * 3600 * 24 * 365, // 1year in ms
            authority_pubkey,
            used_identities: UnorderedSet::new(StorageKey::UsedIdentities),
            admin,
        }
    }

    /**********
     * QUERIES
     **********/

    /// returns information about specific token ID
    pub fn sbt(&self, token_id: TokenId) -> Option<Token> {
        self.token_data.get(&token_id).and_then(|t| {
            Some(Token {
                token_id,
                owner_id: t.owner,
                metadata: t.metadata,
            })
        })
    }

    /// Returns total amount of tokens minted by this contract.
    /// Includes possible expired tokens and revoked tokens.
    // TODO: maybe we will want to use u64 as a return type? But that will break the NFT interface
    // .... nft interface is using U128 anyway
    pub fn nft_total_supply(&self) -> U64 {
        U64(self.sbt_total_supply())
    }

    /// Query sbt tokens by owner
    /// `from_index` and `limit` are not used - one account can have max one sbt.
    // TODO: nft uses U128 instead of U64 ... but it's really not needed.
    #[allow(unused_variables)]
    pub fn nft_tokens_for_owner(
        &self,
        account: AccountId,
        from_index: Option<U64>,
        limit: Option<u64>,
    ) -> Vec<Token> {
        if let Some(t) = self.balances.get(&account) {
            return vec![Token {
                token_id: t,
                owner_id: account,
                metadata: self.token_data.get(&t).unwrap().metadata,
            }];
        }
        return Vec::new();
    }

    /// alias to sbt_supply_for_owner but returns number as a string instead
    pub fn nft_supply_for_owner(&self, account: AccountId) -> U64 {
        self.sbt_supply_for_owner(account).into()
    }

    // SBT Query version //

    pub fn sbt_total_supply(&self) -> u64 {
        self.next_token_id - 1
    }

    /// returns total supply of non revoked SBTs for a given owner.
    pub fn sbt_supply_for_owner(&self, account: AccountId) -> u64 {
        if self.balances.contains_key(&account) {
            1
        } else {
            0
        }
    }

    /**********
     * FUNCTIONS
     **********/

    /// Soulbound transfer implementation.
    /// returns false if caller is not a SBT holder.
    #[payable]
    pub fn sbt_transfer(&mut self, receiver: AccountId) -> bool {
        let owner = env::predecessor_account_id();

        if let Some(sbt) = self.balances.get(&owner) {
            self.balances.remove(&owner);
            self.balances.insert(&receiver, &sbt);
            let mut t = self.token_data.get(&sbt).unwrap();
            t.owner = receiver;
            self.token_data.insert(&sbt, &t);
            return true;
        }
        return false;
    }

    pub fn admin_change_authority(&mut self, authority_pubkey: String) {
        require!(self.admin == env::predecessor_account_id(), "not an admin");
        self.authority_pubkey = base64::decode(authority_pubkey)
            .expect("authority_pubkey is not a valid standard base64");
    }

    /**********
     * ADMIN
     **********/

    /// Mints a new SBT for the transaction signer.
    /// @claim_b64: standard base64 borsh serialized Claim (same bytes as used for the claim signature)
    /// If `metadata.expires_at` is None then we set it to ` now+self.ttl`.
    /// Panics if `metadata.expires_at > now+self.ttl`.
    #[handle_result]
    pub fn sbt_mint(&mut self, claim_b64: String, claim_sig: String) -> Result<TokenId, CtrError> {
        let _sig = b64_decode("claim_sig", claim_sig)?;
        // // TODO: check signature

        // match ed25519::PublicKey::from_bytes(&public_key.0) {
        //     Err(_) => false,
        //     Ok(public_key) => public_key.verify(data, signature).is_ok(),
        // }

        let claim_bytes = b64_decode("claim_b64", claim_b64)?; //.expect("claim_b64 is not a valid standard base64");
                                                               // let claim = Claim::deserialize(&mut &claim_bytes[..])
        let claim = Claim::try_from_slice(&claim_bytes)
            .map_err(|_| CtrError::Borsh("claim".to_string()))?;
        let now = env::block_timestamp() / SECOND;
        if claim.timestamp <= now && now - self.claim_ttl < claim.timestamp {
            return Err(CtrError::BadRequest("claim expired".to_string()));
        }
        if claim.claimer != env::signer_account_id() {
            return Err(CtrError::BadRequest(
                "claimer is not the transaction signer".to_string(),
            ));
        }

        // TODO: check if claimer and external_id are not yet registered and issue SBT

        if self.used_identities.contains(&claim.external_id) {
            return Err(CtrError::DuplicatedID("external_id".to_string()));
        }
        self.used_identities.insert(&claim.external_id);

        if self.balances.contains_key(&claim.claimer) {
            return Err(CtrError::DuplicatedID(
                "receiver already has a SBT".to_string(),
            ));
        }

        let now_ms = env::block_timestamp_ms();
        let metadata = TokenMetadata {
            issued_at: Some(now_ms),
            expires_at: Some(now_ms + self.sbt_ttl_ms),
            reference: None,
            reference_hash: None,
        };

        let token_id = self.next_token_id;
        self.next_token_id += 1;
        self.balances.insert(&claim.claimer, &token_id);
        let event = Events::SbtMint(vec![SbtMintLog {
            owner: claim.claimer.to_string(),
            tokens: vec![token_id],
            memo: None,
        }]);
        emit_event(event);
        self.token_data.insert(
            &token_id,
            &TokenData {
                owner: claim.claimer,
                metadata,
            },
        );
        Ok(token_id)
    }

    // TODO: remove
    // For testing purposes ONLY.
    // NOTE: idenity relationshipt to the issuer is not checked, so it's possible to remove any idenity.
    pub fn sbt_remove(&mut self, identity: String) {
        let claimer = env::predecessor_account_id();
        self.used_identities.remove(&identity);
        self.balances.remove(&claimer);
    }

    // TODO:
    // - fn sbt_renew

    /**********
     * INTERNAL
     **********/
}

#[near_bindgen]
impl SBTMetadata for Contract {
    fn sbt_metadata(&self) -> SBTContractMetadata {
        self.metadata.get().unwrap()
    }
}

type CtrResult<T> = Result<T, CtrError>;

#[derive(BorshSerialize, BorshDeserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq))]
pub struct Claim {
    claimer: AccountId,
    external_id: String,
    /// unix time (seconds) when the claim was signed
    timestamp: u64,
}

fn b64_decode(arg: &str, data: String) -> CtrResult<Vec<u8>> {
    return base64::decode(data).map_err(|e| CtrError::B64Err {
        arg: arg.to_string(),
        err: e,
    });
}
