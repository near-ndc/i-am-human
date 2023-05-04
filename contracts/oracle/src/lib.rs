use ed25519_dalek::{PublicKey, Signature, Verifier, PUBLIC_KEY_LENGTH};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, UnorderedSet};
use near_sdk::{
    env, near_bindgen, require, AccountId, Balance, Gas, PanicOnDefault, Promise, PromiseError,
};

use sbt::*;

// TODO
// use near_sdk::bs58 -- use public key in the base58 format

pub use crate::errors::*;
pub use crate::interfaces::*;
pub use crate::storage::*;
pub use crate::util::*;

mod errors;
mod interfaces;
mod storage;
mod util;

pub const CLASS: ClassId = 1;
// Total storage deposit cost
pub const MINT_TOTAL_COST: Balance = MINT_COST + MILI_NEAR;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    // contract metadata
    pub metadata: LazyOption<ContractMetadata>,

    /// SBT registry
    pub registry: AccountId,

    /// max duration (in seconds) a claim is valid for processing
    pub claim_ttl: u64,
    /// SBT ttl until expire in miliseconds (expire=issue_time+sbt_ttl)
    pub sbt_ttl_ms: u64,
    /// ed25519 pub key (could be same as a NEAR pub key)
    pub authority_pubkey: [u8; PUBLIC_KEY_LENGTH], // Vec<u8>,
    pub used_identities: UnorderedSet<Vec<u8>>,

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
            metadata: LazyOption::new(StorageKey::ContractMetadata, Some(&metadata)),
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

    // all SBT queries should be done through registry

    /*********************
     * NFT compatibility */

    /**********
     * FUNCTIONS
     **********/

    /// Mints a new SBT for the transaction signer.
    /// @claim_b64: standard base64 borsh serialized Claim (same bytes as used for the claim signature)
    /// If `metadata.expires_at` is None then we set it to ` now+self.ttl`.
    /// Panics if `metadata.expires_at > now+self.ttl`.
    // TODO: update result to return TokenId
    #[handle_result]
    #[payable]
    pub fn sbt_mint(
        &mut self,
        claim_b64: String,
        claim_sig: String,
        memo: Option<String>,
    ) -> Result<Promise, CtrError> {
        require!(
            env::attached_deposit() == MINT_TOTAL_COST,
            "Requires attached deposit of exactly 0.008 NEAR"
        );

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
        let external_id = normalize_external_id(claim.external_id)?;

        if self.used_identities.contains(&external_id) {
            return Err(CtrError::DuplicatedID("external_id".to_string()));
        }

        let now_ms = env::block_timestamp_ms();
        let metadata = TokenMetadata {
            class: CLASS,
            issued_at: Some(now_ms),
            expires_at: Some(now_ms + self.sbt_ttl_ms),
            reference: None,
            reference_hash: None,
        };

        self.used_identities.insert(&external_id);

        if let Some(memo) = memo {
            env::log_str(&format!("SBT mint memo: {}", memo));
        }

        let result = ext_registry::ext(self.registry.clone())
            .with_attached_deposit(MINT_COST)
            .with_static_gas(MINT_GAS)
            .sbt_mint(vec![(claim.claimer, vec![metadata])])
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(Gas::ONE_TERA * 3)
                    .sbt_mint_callback(),
            );

        Ok(result)
    }

    #[private]
    pub fn sbt_mint_callback(
        &mut self,
        #[callback_result] last_result: Result<TokenId, PromiseError>,
    ) -> Option<TokenId> {
        if last_result.is_ok() {
            return last_result.ok();
        }
        None
    }

    /**********
     * ADMIN
     **********/

    /// @authority: pubkey used to verify claim signature
    pub fn admin_change_authority(&mut self, authority: String) {
        self.assert_admin();
        self.authority_pubkey = pubkey_from_b64(authority);
    }

    pub fn add_admin(&mut self, admin: AccountId) {
        self.assert_admin();
        self.admins.insert(&admin);
    }

    #[inline]
    fn assert_admin(&self) {
        require!(
            self.admins.contains(&env::predecessor_account_id()),
            "not an admin"
        );
    }

    // TODO:
    // - fn sbt_renew
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
impl SBTContract for Contract {
    fn sbt_metadata(&self) -> ContractMetadata {
        self.metadata.get().unwrap()
    }
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
            .attached_deposit(MINT_TOTAL_COST)
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

    // TODO: find out how to test out of gas.
    /*
    #[test]
    #[should_panic(expected = "todo")]
    fn mint_not_enough_gas() {
        let signer = acc_claimer();
        let (mut ctx, mut ctr, k) = setup(&signer, &acc_u1());

        ctx.prepaid_gas = MINT_GAS - Gas(1);
        testing_env!(ctx);
        let (_, c_str, sig) = mk_claim_sign(start() / SECOND, "0x1a", &k);
        let _ = ctr.sbt_mint(c_str.clone(), sig.clone(), None);
    }
    */

    #[test]
    #[should_panic(expected = "Requires attached deposit of exactly 0.008 NEAR")]
    fn mint_not_enough_storage_deposit() {
        let signer = acc_claimer();
        let (mut ctx, mut ctr, k) = setup(&signer, &acc_u1());

        // fail: not enough storage deposit
        ctx.attached_deposit = MINT_TOTAL_COST - 1;
        testing_env!(ctx);
        let (_, c_str, sig) = mk_claim_sign(start() / SECOND, "0x1a", &k);
        let _ = ctr
            .sbt_mint(c_str.clone(), sig.clone(), None)
            .expect("must panic");
    }

    #[test]
    fn flow1() {
        let signer = acc_claimer();
        let predecessor = acc_u1();
        let (mut ctx, mut ctr, k) = setup(&signer, &predecessor);
        // fail: tx signer is not claimer
        ctx.signer_account_id = acc_u1();
        testing_env!(ctx.clone());
        let (_, c_str, sig) = mk_claim_sign(start() / SECOND, "0x1a", &k);
        match ctr.sbt_mint(c_str.clone(), sig.clone(), None) {
            Err(CtrError::BadRequest(s)) => assert_eq!(s, "claimer is not the transaction signer"),

            Err(error) => panic!("expected BadRequest, got: {:?}", error),
            Ok(_) => panic!("expected BadRequest, got: Ok"),
        }

        // fail: claim_ttl passed
        ctx.signer_account_id = signer.clone();
        ctx.block_timestamp = start() + CLAIM_TTL * SECOND;
        testing_env!(ctx.clone());
        match ctr.sbt_mint(c_str.clone(), sig.clone(), None) {
            Err(CtrError::BadRequest(s)) => {
                assert_eq!("claim expired", s, "wrong BadRequest: {}", s)
            }
            Err(error) => panic!("expected BadRequest, got: {:?}", error),
            Ok(_) => panic!("expected BadRequest, got: Ok"),
        }

        // fail: claim_ttl passed way more
        ctx.signer_account_id = signer.clone();
        ctx.block_timestamp = start() + CLAIM_TTL * 10 * SECOND;
        testing_env!(ctx.clone());
        match ctr.sbt_mint(c_str.clone(), sig.clone(), None) {
            Err(CtrError::BadRequest(s)) => {
                assert_eq!("claim expired", s, "wrong BadRequest: {}", s)
            }
            Err(error) => panic!("expected BadRequest, got: {:?}", error),
            Ok(_) => panic!("expected BadRequest, got: Ok"),
        }

        // test case: claim.timestamp can't be in the future
        ctx.block_timestamp = start() - SECOND;
        testing_env!(ctx.clone());
        match ctr.sbt_mint(c_str.clone(), sig.clone(), None) {
            Err(CtrError::BadRequest(s)) => assert_eq!("claim.timestamp in the future", s),
            Err(error) => panic!("expected BadRequest, got: {:?}", error),
            Ok(_) => panic!("expected BadRequest, got: Ok"),
        }

        // should create a SBT for a valid claim
        ctx.block_timestamp = start() + SECOND;
        testing_env!(ctx.clone());
        let resp = ctr.sbt_mint(c_str.clone(), sig.clone(), None);
        assert!(resp.is_ok(), "should accept valid claim");

        // fail: signer already has SBT
        match ctr.sbt_mint(c_str.clone(), sig.clone(), None) {
            Err(CtrError::DuplicatedID(_)) => (),
            Err(error) => panic!("expected DuplicatedID, got: {:?}", error),
            Ok(_) => panic!("expected DuplicatedID, got: Ok"),
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
        let (_, c_str, sig) = mk_claim_sign(start() / SECOND, "0x12", &k);
        let claim_bytes = b64_decode("claim_b64", c_str).unwrap();
        let res = verify_claim(
            &k.public.to_bytes(),
            claim_bytes,
            b64_decode("sig", sig).unwrap(),
        );
        assert!(res.is_ok(), "verification result: {:?}", res);
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

    #[allow(dead_code)]
    // #[test]
    fn sig_deserialization_check() {
        let sig_b64 =
            "o8MGudK9OrdNKVCMhjF7rEv9LangB+PdjxuQ0kgglCskZX7Al4JPrwf7tRlT252kiNpJaGPURgAvAA==";
        let sig_bz = b64_decode("sig", sig_b64.to_string()).unwrap();
        println!("sig len: {}", sig_bz.len());
        Signature::from_bytes(&sig_bz).unwrap();
    }

    #[allow(dead_code)]
    #[test]
    fn claim_deserialization_check() {
        let c = "EQAAAGhhcnJ5ZGhpbGxvbi5uZWFyKgAAADB4YjRiZjBmMjNjNzAyZWZiOGE5ZGE4N2E5NDA5NWUyOGRlM2QyMWNjMyDzAGQAAAAA";
        let c_bz = b64_decode("claim", c.to_string()).unwrap();
        let c = Claim::try_from_slice(&c_bz).unwrap();
        println!("claim: {:?}", c);
        assert_eq!(
            c.external_id, "0xb4bf0f23c702efb8a9da87a94095e28de3d21cc3",
            "deserialization check"
        );
    }
}
