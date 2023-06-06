use ed25519_dalek::{PublicKey, Signature, Verifier, PUBLIC_KEY_LENGTH};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, UnorderedSet};
use near_sdk::serde::Serialize;
use near_sdk::{
    env, near_bindgen, require, AccountId, Balance, Gas, PanicOnDefault, Promise, PromiseError,
};

use cost::*;
use sbt::*;
use uint::hex;

// TODO
// use near_sdk::bs58 -- use public key in the base58 format

pub use crate::errors::*;
pub use crate::storage::*;
pub use crate::util::*;

mod errors;
mod storage;
mod util;

pub const CLASS_FV_SBT: ClassId = 1;
pub const CLASS_KYC_SBT: ClassId = 2;

// Total storage deposit cost without KYC
pub const MINT_TOTAL_COST: Balance = MINT_COST + MILI_NEAR;
pub const MINT_TOTAL_COST_WITH_KYC: Balance = 2 * MINT_COST + MILI_NEAR;

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
            sbt_ttl_ms: 1000 * 3600 * 24 * 548, // 1.5years in ms
            authority_pubkey: pubkey_from_b64(authority),
            used_identities: UnorderedSet::new(StorageKey::UsedIdentities),
            admins,
        }
    }

    /**********
     * QUERIES
     **********/

    /// Checks if the given id was already used to mint an sbt
    pub fn is_used_identity(&self, external_id: String) -> bool {
        let normalised_id = normalize_external_id(external_id).expect("failed to normalize id");
        self.used_identities.contains(&normalised_id)
    }

    #[inline]
    pub fn get_required_sbt_mint_deposit(is_verified_kyc: bool) -> Balance {
        if is_verified_kyc {
            return MINT_TOTAL_COST_WITH_KYC;
        };
        MINT_TOTAL_COST
    }

    // all SBT queries should be done through registry

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
        let user = env::signer_account_id();
        if !is_supported_account(user.as_ref().chars()) {
            return Err(CtrError::BadRequest(
                "only root and implicit accounts are allowed to get SBT".to_owned(),
            ));
        }

        let sig = b64_decode("claim_sig", claim_sig)?;
        let claim_bytes = b64_decode("claim_b64", claim_b64)?;
        // let claim = Claim::deserialize(&mut &claim_bytes[..])
        let claim = Claim::try_from_slice(&claim_bytes)
            .map_err(|_| CtrError::Borsh("claim".to_string()))?;

        if claim.verified_kyc {
            require!(
                env::attached_deposit() == MINT_TOTAL_COST_WITH_KYC,
                "Requires attached deposit of exactly 0.015 NEAR"
            );
        } else {
            require!(
                env::attached_deposit() == MINT_TOTAL_COST,
                "Requires attached deposit of exactly 0.008 NEAR"
            );
        }

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

        if claim.claimer != user {
            return Err(CtrError::BadRequest(
                "claimer is not the transaction signer".to_string(),
            ));
        }
        let external_id = normalize_external_id(claim.external_id)?;

        if self.used_identities.contains(&external_id) {
            return Err(CtrError::DuplicatedID("external_id".to_string()));
        }

        let now_ms = env::block_timestamp_ms();
        let mut tokens_metadata: Vec<TokenMetadata> = Vec::new();
        tokens_metadata.push(TokenMetadata {
            class: CLASS_FV_SBT,
            issued_at: Some(now_ms),
            expires_at: Some(now_ms + self.sbt_ttl_ms),
            reference: None,
            reference_hash: None,
        });
        //KYC token to be minted. Class is set to `2` to differentiate the token
        if claim.verified_kyc {
            tokens_metadata.push(TokenMetadata {
                class: CLASS_KYC_SBT,
                issued_at: Some(now_ms),
                expires_at: Some(now_ms + self.sbt_ttl_ms),
                reference: None,
                reference_hash: None,
            });
        }

        self.used_identities.insert(&external_id);

        if let Some(memo) = memo {
            env::log_str(&format!("SBT mint memo: {}", memo));
        }

        let result = ext_registry::ext(self.registry.clone())
            .with_attached_deposit(Self::get_required_sbt_mint_deposit(claim.verified_kyc))
            .with_static_gas(MINT_GAS)
            .sbt_mint(vec![(claim.claimer, tokens_metadata)])
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(Gas::ONE_TERA * 3)
                    .sbt_mint_callback(hex::encode(external_id)),
            );

        Ok(result)
    }

    // We use our own result type, because NEAR stopped to support standard `Result` return
    // type without `handle_result`. With `handle_result` we would need to make an ugly wrap
    // to always return Ok at the outer layer:
    //     Result<Result<TokenId, &str>, near_sdk::Abort>
    // The problem is that NEAR explorer considers transaction successfull if the last receipt
    // didn't panic. However, if we do so, then we can't panic in this function in order to
    // preserve the state change (rollback for `used_identities`).
    // Other solution (probably the right one) is to schedule another callback to "self" which
    // will panic.
    // Ideally, though, NEAR will start considering Result types again.
    #[private]
    pub fn sbt_mint_callback(
        &mut self,
        external_id: String,
        #[callback_result] last_result: Result<Vec<TokenId>, PromiseError>,
    ) -> CallbackResult<TokenId, &str> {
        match last_result {
            Ok(v) => CallbackResult::Ok(v[0]),
            Err(_) => {
                // registry mint failed, need to rollback. We can't panic here in order to
                // preserve state change.
                // We are safe to remove the external identity, because we only call registry
                // if the external_id was not used before.
                self.used_identities
                    .remove(&hex::decode(external_id).unwrap());
                CallbackResult::Err("registry.sbt_mint failed")
            }
        }
    }

    // Revokes the provided token list from the registry.
    // Must be called by an admin
    pub fn sbt_revoke(&mut self, tokens: Vec<TokenId>, burn: bool) -> Promise {
        self.assert_admin();
        ext_registry::ext(self.registry.clone())
            .with_static_gas(MINT_GAS)
            .sbt_revoke(tokens, burn)
    }

    /**********
     * ADMIN
     **********/

    /* for testing the callback
        #[payable]
        pub fn admin_mint(&mut self, receipient: AccountId, external_id: String) -> Promise {
            let external_id = normalize_external_id(external_id).ok().unwrap();
            let now = env::block_timestamp_ms();
            let metadata = TokenMetadata {
                class: 2,
                issued_at: Some(now),
                expires_at: Some(now + self.sbt_ttl_ms),
                reference: None,
                reference_hash: None,
            };
            ext_registry::ext(self.registry.clone())
                .with_attached_deposit(MINT_COST)
                .with_static_gas(MINT_GAS)
                .sbt_mint(vec![(receipient, vec![metadata])])
                .then(
                    Self::ext(env::current_account_id())
                        .with_static_gas(Gas::ONE_TERA * 3)
                        .sbt_mint_callback(hex::encode(external_id)),
                )
        }
    */

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

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub enum CallbackResult<T, E> {
    Ok(T),
    Err(E),
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod checks;

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

    fn acc_implicit() -> AccountId {
        "061b1dd17603213b00e1a1e53ba060ad427cef4887bd34a5e0ef09010af23b0a"
            .parse()
            .unwrap()
    }

    // wrong implicit account
    fn acc_bad_implicit() -> AccountId {
        "061b1dd17603213b00e1a1e53ba060ad427cef4887bd34a5e0ef09010af23b0"
            .parse()
            .unwrap()
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
    fn mk_claim(timestamp: u64, external_id: &str, is_verified_kyc: bool) -> Claim {
        Claim {
            claimer: acc_claimer(),
            external_id: external_id.to_string(),
            timestamp,
            verified_kyc: is_verified_kyc,
        }
    }

    // returns b64 serialized claim and signature
    fn sign_claim(c: &Claim, k: &Keypair) -> (String, String) {
        let c_bz = c.try_to_vec().unwrap();
        let sig = k.sign(&c_bz);
        let sig_bz = sig.to_bytes();
        (b64_encode(c_bz), b64_encode(sig_bz.to_vec()))
    }

    fn mk_claim_sign(
        timestamp: u64,
        external_id: &str,
        k: &Keypair,
        is_verified_kyc: bool,
    ) -> (Claim, String, String) {
        let c = mk_claim(timestamp, external_id, is_verified_kyc);
        let (c_str, sig) = sign_claim(&c, &k);
        return (c, c_str, sig);
    }

    fn assert_bad_request(resp: Result<Promise, CtrError>, expected_msg: &str) {
        match resp {
            Err(CtrError::BadRequest(s)) => {
                assert_eq!(s, expected_msg)
            }
            Err(error) => panic!("expected BadRequest, got: {:?}", error),
            Ok(_) => panic!("expected BadRequest, got: Ok"),
        }
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
        let (_, c_str, sig) = mk_claim_sign(start() / SECOND, "0x1a", &k, false);
        let _ = ctr
            .sbt_mint(c_str.clone(), sig.clone(), None)
            .expect("must panic");
    }

    #[test]
    #[should_panic(expected = "Requires attached deposit of exactly 0.015 NEAR")]
    fn mint_with_kyc_not_enough_storage_deposit() {
        let signer = acc_claimer();
        let (mut ctx, mut ctr, k) = setup(&signer, &acc_u1());

        // fail: not enough storage deposit
        ctx.attached_deposit = MINT_TOTAL_COST_WITH_KYC - 1;
        testing_env!(ctx);
        let (_, c_str, sig) = mk_claim_sign(start() / SECOND, "0x1a", &k, true);
        let _ = ctr
            .sbt_mint(c_str.clone(), sig.clone(), None)
            .expect("must panic");
    }

    #[test]
    fn mint_no_root_account() {
        let signer: AccountId = "user1".parse().unwrap();
        let predecessor: AccountId = "some.other".parse().unwrap();
        let (mut ctx, mut ctr, k) = setup(&signer, &predecessor);

        let (_, c_str, sig) = mk_claim_sign(start() / SECOND, "0x1a", &k, false);
        assert_bad_request(
            ctr.sbt_mint(c_str.clone(), sig.clone(), None),
            "only root and implicit accounts are allowed to get SBT",
        );

        ctx.signer_account_id = "sub.user1.near".parse().unwrap();
        testing_env!(ctx.clone());
        assert_bad_request(
            ctr.sbt_mint(c_str.clone(), sig.clone(), None),
            "only root and implicit accounts are allowed to get SBT",
        );

        ctx.signer_account_id = "sub.sub.user1.near".parse().unwrap();
        testing_env!(ctx.clone());
        assert_bad_request(
            ctr.sbt_mint(c_str.clone(), sig.clone(), None),
            "only root and implicit accounts are allowed to get SBT",
        );

        ctx.signer_account_id = "a123".parse().unwrap();
        testing_env!(ctx.clone());
        assert_bad_request(
            ctr.sbt_mint(c_str.clone(), sig.clone(), None),
            "only root and implicit accounts are allowed to get SBT",
        );

        ctx.signer_account_id = acc_bad_implicit();
        testing_env!(ctx.clone());
        assert_bad_request(
            ctr.sbt_mint(c_str.clone(), sig.clone(), None),
            "only root and implicit accounts are allowed to get SBT",
        );

        ctx.signer_account_id = acc_implicit();
        testing_env!(ctx.clone());
        assert_bad_request(
            ctr.sbt_mint(c_str.clone(), sig.clone(), None),
            "claimer is not the transaction signer",
        );
    }

    #[test]
    fn flow1() {
        let signer = acc_claimer();
        let predecessor = acc_u1();
        let (mut ctx, mut ctr, k) = setup(&signer, &predecessor);
        // fail: tx signer is not claimer
        ctx.signer_account_id = acc_u1();
        testing_env!(ctx.clone());
        let (_, c_str, sig) = mk_claim_sign(start() / SECOND, "0x1a", &k, false);
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
        let (_, c_str, sig) = mk_claim_sign(start() / SECOND, "0x12", &k, false);
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
        let c = mk_claim(1677621259142, "some_111#$!", false);
        let claim_bz = c.try_to_vec().unwrap();
        let claim_str = b64_encode(claim_bz);
        let claim2 = checks::tests::deserialize_claim(&claim_str);
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
}
