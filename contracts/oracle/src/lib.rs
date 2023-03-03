use ed25519_dalek::{PublicKey, Signature, Verifier, PUBLIC_KEY_LENGTH};
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
    pub metadata: LazyOption<ContractMetadata>,

    pub next_token_id: TokenId,
    /// max duration (in seconds) a claim is valid for processing
    pub claim_ttl: u64,
    /// SBT ttl until expire in miliseconds (expire=issue_time+sbt_ttl)
    pub sbt_ttl_ms: u64,
    /// ed25519 pub key (could be same as a NEAR pub key)
    pub authority_pubkey: [u8; PUBLIC_KEY_LENGTH], // Vec<u8>,
    pub used_identities: UnorderedSet<String>,

    /// used for backend key rotation
    pub admins: UnorderedSet<AccountId>,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    /// @authority: base64 of authority pub key used for claim signature authorization.
    /// @metadata: NFT like metadata about the contract.
    /// @registry: the SBT registry responsable for the "soul transfer".
    /// @claim_ttl: max duration (in seconds) a claim is valid for processing.
    ///   If zero default (1 day) is used.
    #[init]
    pub fn new(
        authority: String,
        metadata: ContractMetadata,
        registry: AccountId,
        claim_ttl: u64,
        admin: AccountId,
    ) -> Self {
        let claim_ttl = if claim_ttl == 0 {
            3600 * 24 // 1 day
        } else {
            claim_ttl
        };
        let mut admins = UnorderedSet::new(StorageKey::Admins);
        admins.insert(&admin);
        Self {
            registry,

            balances: LookupMap::new(StorageKey::Balances),
            token_data: LookupMap::new(StorageKey::TokenData),
            metadata: LazyOption::new(StorageKey::ContractMetadata, Some(&metadata)),
            next_token_id: 1,
            claim_ttl,
            sbt_ttl_ms: 1000 * 3600 * 24 * 365, // 1year in ms
            authority_pubkey: pubkey_from_b64(authority),
            used_identities: UnorderedSet::new(StorageKey::UsedIdentities),
            admins,
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
                metadata: t.metadata.v1(),
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
                metadata: self.token_data.get(&t).unwrap().metadata.v1(),
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

    /**********
     * ADMIN
     **********/

    /// Mints a new SBT for the transaction signer.
    /// @claim_b64: standard base64 borsh serialized Claim (same bytes as used for the claim signature)
    /// If `metadata.expires_at` is None then we set it to ` now+self.ttl`.
    /// Panics if `metadata.expires_at > now+self.ttl`.
    #[handle_result]
    pub fn sbt_mint(&mut self, claim_b64: String, claim_sig: String) -> Result<TokenId, CtrError> {
        let sig = b64_decode("claim_sig", claim_sig)?;
        let claim_bytes = b64_decode("claim_b64", claim_b64)?;
        // let claim = Claim::deserialize(&mut &claim_bytes[..])
        let claim = Claim::try_from_slice(&claim_bytes)
            .map_err(|_| CtrError::Borsh("claim".to_string()))?;

        verify_claim(&self.authority_pubkey, claim_bytes, sig)?;

        let now = env::block_timestamp() / SECOND;
        if claim.timestamp > now {
            return Err(CtrError::BadRequest(
                "claim.timestamp in the future".to_string(),
            ));
        }
        if now >= claim.timestamp + self.claim_ttl {
            return Err(CtrError::BadRequest("claim expired".to_string()));
        }

        if claim.claimer != env::signer_account_id() {
            return Err(CtrError::BadRequest(
                "claimer is not the transaction signer".to_string(),
            ));
        }
        if self.used_identities.contains(&claim.external_id) {
            return Err(CtrError::DuplicatedID("external_id".to_string()));
        }
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
        self.used_identities.insert(&claim.external_id);
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
                metadata: metadata.into(),
            },
        );
        Ok(token_id)
    }

    /// @authority: pubkey used to verify claim signature
    pub fn admin_change_authority(&mut self, authority: String) {
        self.assure_admin();
        self.authority_pubkey = pubkey_from_b64(authority);
    }

    pub fn add_admin(&mut self, admin: AccountId) {
        self.assure_admin();
        self.admins.insert(&admin);
    }

    #[inline]
    fn assure_admin(&self) {
        require!(
            self.admins.contains(&env::predecessor_account_id()),
            "not an admin"
        );
    }

    /// remove sbt.
    /// Must match owner with his external_id.
    /// Panics if not on testnet.
    pub fn admin_remove_sbt(&mut self, owner: AccountId, external_id: String) {
        // require!(
        //     str::ends_with(env::current_account_id().as_ref(), "testnet"),
        //     "can only remove sbt on testnet"
        // );
        self.assure_admin();
        self.balances.remove(&owner);
        self.used_identities.remove(&external_id);
    }

    // TODO:
    // - fn sbt_renew
}

fn b64_decode(arg: &str, data: String) -> CtrResult<Vec<u8>> {
    return base64::decode(data).map_err(|e| CtrError::B64Err {
        arg: arg.to_string(),
        err: e,
    });
}

fn pubkey_from_b64(pubkey: String) -> [u8; PUBLIC_KEY_LENGTH] {
    let pk_bz = base64::decode(pubkey).expect("authority_pubkey is not a valid standard base64");
    pk_bz.try_into().expect("authority pubkey must be 32 bytes")
}

fn verify_claim(
    pubkey: &[u8; PUBLIC_KEY_LENGTH],
    claim: Vec<u8>,
    claim_sig: Vec<u8>,
) -> Result<(), CtrError> {
    let pk = PublicKey::from_bytes(pubkey).unwrap();
    let sig = match Signature::from_bytes(&claim_sig) {
        Ok(sig) => sig,
        Err(_) => return Err(CtrError::Signature("malformed signature".to_string())),
    };
    pk.verify(&claim, &sig)
        .map_err(|_| CtrError::Signature("invalid signature".to_string()))
}

#[near_bindgen]
impl SBTMetadata for Contract {
    fn sbt_metadata(&self) -> ContractMetadata {
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    extern crate ed25519_dalek;
    extern crate rand;

    use crate::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::{testing_env, VMContext};

    use ed25519_dalek::{Keypair, Signer};
    use rand::rngs::OsRng;

    fn b64_encode(data: Vec<u8>) -> String {
        near_sdk::base64::encode(data)
    }

    fn acc_claimer() -> AccountId {
        "user1.near".parse().unwrap()
    }

    #[allow(dead_code)]
    fn acc_u1() -> AccountId {
        "user2.near".parse().unwrap()
    }

    fn acc_registry() -> AccountId {
        "registry".parse().unwrap()
    }

    fn acc_admin() -> AccountId {
        "admin".parse().unwrap()
    }

    fn start() -> u64 {
        11 * SECOND
    }

    /// SBT claim ttl in seconds
    const CLAIM_TTL: u64 = 2;

    fn setup(signer: &AccountId, predecessor: &AccountId) -> (VMContext, Contract, Keypair) {
        let ctx = VMContextBuilder::new()
            .signer_account_id(signer.clone())
            .predecessor_account_id(predecessor.clone())
            // .attached_deposit(deposit_dec.into())
            .block_timestamp(start())
            .is_view(false)
            .build();

        let mut csprng = OsRng {};
        let keypair: Keypair = Keypair::generate(&mut csprng);
        let ctr = Contract::new(
            b64_encode(keypair.public.to_bytes().to_vec()),
            ContractMetadata {
                spec: STANDARD_NAME.to_string(),
                name: "name".to_string(),
                symbol: "symbol".to_string(),
                icon: None,
                base_uri: None,
                reference: None,
                reference_hash: None,
            },
            acc_registry(),
            CLAIM_TTL,
            acc_admin(),
        );
        testing_env!(ctx.clone());

        return (ctx, ctr, keypair);
    }

    /// @timestamp: in seconds
    fn mk_claim(timestamp: u64, external_id: &str) -> Claim {
        Claim {
            claimer: acc_claimer(),
            external_id: external_id.to_string(),
            timestamp,
        }
    }

    // returns b64 serialized claim and signature
    fn sign_claim(c: &Claim, k: &Keypair) -> (String, String) {
        let c_bz = c.try_to_vec().unwrap();
        let sig = k.sign(&c_bz);
        let sig_bz = sig.to_bytes();
        (b64_encode(c_bz), b64_encode(sig_bz.to_vec()))
    }

    fn mk_claim_sign(timestamp: u64, external_id: &str, k: &Keypair) -> (Claim, String, String) {
        let c = mk_claim(timestamp, external_id);
        let (c_str, sig) = sign_claim(&c, &k);
        return (c, c_str, sig);
    }

    #[test]
    fn flow1() {
        let signer = acc_claimer();
        let predecessor = acc_u1();
        let (mut ctx, mut ctr, k) = setup(&signer, &predecessor);

        // fail: tx signer is not claimer
        let (_, c_str, sig) = mk_claim_sign(start() / SECOND, "id1", &k);
        ctx.signer_account_id = acc_u1();
        testing_env!(ctx.clone());
        match ctr.sbt_mint(c_str.clone(), sig.clone()) {
            Err(CtrError::BadRequest(s)) => assert_eq!(s, "claimer is not the transaction signer"),
            resp @ _ => panic!("expected BadRequest, got: {:?}", resp),
        }

        // fail: claim_ttl passed
        ctx.signer_account_id = signer.clone();
        ctx.block_timestamp = start() + CLAIM_TTL * SECOND;
        testing_env!(ctx.clone());
        match ctr.sbt_mint(c_str.clone(), sig.clone()) {
            Err(CtrError::BadRequest(s)) => {
                assert_eq!("claim expired", s, "wrong BadRequest: {}", s)
            }
            resp @ _ => panic!("expected BadRequest, got: {:?}", resp),
        }

        // fail: claim_ttl passed way more
        ctx.signer_account_id = signer.clone();
        ctx.block_timestamp = start() + CLAIM_TTL * 10 * SECOND;
        testing_env!(ctx.clone());
        match ctr.sbt_mint(c_str.clone(), sig.clone()) {
            Err(CtrError::BadRequest(s)) => {
                assert_eq!("claim expired", s, "wrong BadRequest: {}", s)
            }
            resp @ _ => panic!("expected BadRequest, got: {:?}", resp),
        }

        // test case: claim.timestamp can't be in the future
        ctx.block_timestamp = start() - SECOND;
        testing_env!(ctx.clone());
        match ctr.sbt_mint(c_str.clone(), sig.clone()) {
            Err(CtrError::BadRequest(s)) => assert_eq!("claim.timestamp in the future", s),
            resp @ _ => panic!("expected BadRequest, got: {:?}", resp),
        }

        // should create a SBT for a valid claim
        ctx.block_timestamp = start() + SECOND;
        testing_env!(ctx.clone());
        let resp = ctr.sbt_mint(c_str.clone(), sig.clone());
        assert!(resp.is_ok(), "should accept valid claim, {:?}", resp);

        // fail: signer already has SBT
        match ctr.sbt_mint(c_str.clone(), sig.clone()) {
            Err(CtrError::DuplicatedID(_)) => (),
            resp @ _ => panic!("DuplicatedID, got: {:?}", resp),
        }
    }

    #[test]
    fn test_pubkey() {
        let pk_bytes = pubkey_from_b64("kSj7W/TdN9RGLgdJA8ac7i/WdQdm2lwQ1IPGlO1L3xc=".to_string());
        assert_ne!(pk_bytes[0], 0);
    }

    #[test]
    fn test_pubkey_sig() {
        let mut csprng = OsRng {};
        let k = Keypair::generate(&mut csprng);
        let (_, c_str, sig) = mk_claim_sign(start() / SECOND, "id1", &k);
        let claim_bytes = b64_decode("claim_b64", c_str).unwrap();
        let res = verify_claim(
            &k.public.to_bytes(),
            claim_bytes,
            b64_decode("sig", sig).unwrap(),
        );
        assert!(res.is_ok(), "verification result: {:?}", res);
    }

    #[test]
    fn test_sig_deserialization() {
        let sig_b64 =
            "o8MGudK9OrdNKVCMhjF7rEv9LangB+PdjxuQ0kgglCskZX7Al4JPrwf7tRlT252kiNpJaGPURgAvAA==";
        let sig_bz = b64_decode("sig", sig_b64.to_string()).unwrap();
        println!("sig len: {}", sig_bz.len());
        Signature::from_bytes(&sig_bz).unwrap();
    }

    #[test]
    fn claim_serialization() {
        let c = mk_claim(1677621259142, "some_111#$!");
        let claim_bz = c.try_to_vec().unwrap();
        let claim_str = b64_encode(claim_bz);
        let claim2_bz = b64_decode("claim", claim_str).unwrap();
        let claim2 = Claim::try_from_slice(&claim2_bz).unwrap();
        assert_eq!(c, claim2, "serialization should work");
    }
}
