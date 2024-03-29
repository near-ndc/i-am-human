use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, LookupMap, UnorderedSet};
use near_sdk::serde::Serialize;
use near_sdk::{
    env, near_bindgen, require, AccountId, Balance, Gas, PanicOnDefault, Promise, PromiseError,
};

#[allow(unused_imports)]
use near_sdk::__private::schemars;

use cost::*;
use sbt::*;
use uint::hex;

// TODO
// use near_sdk::bs58 -- use public key in the base58 format

pub use crate::errors::*;
pub use crate::storage::*;
pub use crate::util::*;

mod errors;
mod migrate;
mod storage;
mod util;

pub const CLASS_FV_SBT: ClassId = 1;
pub const CLASS_KYC_SBT: ClassId = 2;

// Total storage deposit cost without KYC
pub const MINT_TOTAL_COST: Balance = mint_deposit(1);
pub const MINT_TOTAL_COST_WITH_KYC: Balance = mint_deposit(2);

pub const ELECTIONS_START: u64 = 1693612799000; // Fri, 1 Sep 2023 23:59:59 UTC in ms
pub const ELECTIONS_END: u64 = 1695427199000; // Fri, 22 Sep 2023 23:59:59 UTC in ms

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
    pub authority_pubkey: [u8; PUBLIC_KEY_LEN], // Vec<u8>,
    pub used_identities: UnorderedSet<Vec<u8>>,

    /// used for backend key rotation
    pub admins: UnorderedSet<AccountId>,

    /// class metadata
    pub class_metadata: LookupMap<ClassId, ClassMetadata>,
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
            class_metadata: LookupMap::new(StorageKey::ClassMetadata),
        }
    }

    /**********
     * QUERIES
     *
     * Note: all SBT queries should be done through registry
     **********/

    /// Returns list of admins
    pub fn get_admins(&self) -> Vec<AccountId> {
        self.admins.iter().collect()
    }

    #[inline]
    pub fn required_sbt_mint_deposit(is_verified_kyc: bool) -> Balance {
        if is_verified_kyc {
            return MINT_TOTAL_COST_WITH_KYC;
        };
        MINT_TOTAL_COST
    }

    /// Checks if the given id was already used to mint an sbt
    pub fn is_used_identity(&self, external_id: String) -> bool {
        let normalised_id = normalize_external_id(external_id).expect("failed to normalize id");
        self.used_identities.contains(&normalised_id)
    }

    /**********
     * FUNCTIONS
     **********/

    /// Mints a new SBT for the transaction signer.
    /// @claim_b64: standard base64 borsh serialized Claim (same bytes as used for the claim signature).
    /// @claim_sig: standard base64 serialized ed25519 signature.
    /// If `metadata.expires_at` is None then we set it to ` now+self.ttl`.
    /// Panics if `metadata.expires_at > now+self.ttl`.
    /// Throws an error if trying to mint during the elections period.
    // TODO: update result to return TokenId
    #[handle_result]
    #[payable]
    pub fn sbt_mint(
        &mut self,
        claim_b64: String,
        claim_sig: String,
        memo: Option<String>,
    ) -> Result<Promise, CtrError> {
        let now_ms = env::block_timestamp_ms();
        let this_acc = env::current_account_id();
        // only stop in prod
        if this_acc.as_str().ends_with("i-am-human.near")
            && now_ms > ELECTIONS_START
            && now_ms <= ELECTIONS_END
        {
            return Err(CtrError::BadRequest(
                "IAH SBT cannot be mint during the elections period".to_owned(),
            ));
        }

        let user = env::signer_account_id();
        if !is_supported_account(user.as_ref().chars()) {
            return Err(CtrError::BadRequest(
                "only root and implicit accounts are allowed to get SBT".to_owned(),
            ));
        }

        let claim_bytes = b64_decode("claim_b64", claim_b64)?;
        let claim = Claim::try_from_slice(&claim_bytes)
            .map_err(|_| CtrError::Borsh("claim".to_string()))?;
        let signature = b64_decode("claim_sig", claim_sig)?;
        verify_claim(&signature, &claim_bytes, &self.authority_pubkey)?;

        let storage_deposit = Self::required_sbt_mint_deposit(claim.verified_kyc);
        require!(
            env::attached_deposit() >= storage_deposit,
            format!(
                "Requires attached deposit at least {} yoctoNEAR",
                storage_deposit
            )
        );
        let num_tokens = if claim.verified_kyc { 2 } else { 1 };

        let now = now_ms / 1000;
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
            .with_attached_deposit(storage_deposit)
            .with_static_gas(calculate_mint_gas(num_tokens))
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
            .with_static_gas(MINT_GAS * tokens.len() as u64)
            .sbt_revoke(tokens, burn)
    }

    /**********
     * ADMIN
     **********/

    /* for testing the callback
        #[payable]
        pub fn admin_mint(&mut self, recipient: AccountId, external_id: String) -> Promise {
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
                .sbt_mint(vec![(recipient, vec![metadata])])
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

    pub fn remove_admin(&mut self, admin: AccountId) {
        self.assert_admin();
        self.admins.remove(&admin);
    }

    #[inline]
    fn assert_admin(&self) {
        require!(
            self.admins.contains(&env::predecessor_account_id()),
            "not an admin"
        );
    }

    /// Allows admin to update class metadata.
    /// Panics if not admin or the class is not found (Currently oracle only supports classes: [1,2])
    #[handle_result]
    pub fn set_class_metadata(
        &mut self,
        class: ClassId,
        metadata: ClassMetadata,
    ) -> Result<(), CtrError> {
        self.assert_admin();
        if class != 1 && class != 2 {
            return Err(CtrError::BadRequest("class not found".to_string()));
        }
        self.class_metadata.insert(&class, &metadata);
        Ok(())
    }

    /// Alows admin to mint SBTs with a of the `class_id` to the provided list of pairs:
    /// `(recipient_account, expire_timestamp_ms)`.
    /// Panics if not called by an admin or the attached deposit is insufficient.
    #[payable]
    pub fn admin_mint(
        &mut self,
        mint_data: Vec<(AccountId, u64)>,
        class: ClassId,
        memo: Option<String>,
    ) -> Promise {
        self.assert_admin();

        let num_tokens = mint_data.len();
        let deposit = env::attached_deposit();
        let required_deposit = mint_deposit(num_tokens);
        require!(
            deposit >= required_deposit,
            format!("Requires min {}yoctoNEAR storage deposit", required_deposit)
        );
        require!(
            class == CLASS_FV_SBT || class == CLASS_KYC_SBT,
            "wrong request, class must be either 1 (FV) or 2 (KYC)"
        );

        if deposit > required_deposit {
            Promise::new(env::predecessor_account_id()).transfer(deposit - required_deposit);
        }

        let now: u64 = env::block_timestamp_ms();
        let mut tokens_metadata: Vec<(AccountId, Vec<TokenMetadata>)> =
            Vec::with_capacity(num_tokens);
        for (acc, end) in mint_data {
            tokens_metadata.push((
                acc,
                vec![TokenMetadata {
                    class,
                    issued_at: Some(now),
                    expires_at: Some(end),
                    reference: None,
                    reference_hash: None,
                }],
            ));
        }

        if let Some(memo) = memo {
            env::log_str(&format!("SBT mint memo: {}", memo));
        }

        ext_registry::ext(self.registry.clone())
            .with_attached_deposit(required_deposit)
            .with_static_gas(calculate_mint_gas(num_tokens))
            .sbt_mint(tokens_metadata)
    }

    // TODO:
    // - fn sbt_renew
}

#[near_bindgen]
impl SBTIssuer for Contract {
    fn sbt_metadata(&self) -> ContractMetadata {
        self.metadata.get().unwrap()
    }
    /// Returns `ClassMetadata` by class. Returns none if the class is not found.
    fn sbt_class_metadata(&self, class: ClassId) -> Option<ClassMetadata> {
        self.class_metadata.get(&class)
    }
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(
    not(target_arch = "wasm32"),
    derive(schemars::JsonSchema, borsh::BorshSchema)
)]
pub enum CallbackResult<T, E> {
    Ok(T),
    Err(E),
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod checks;

#[cfg(all(test, not(target_arch = "wasm32")))]
pub mod tests {
    use crate::*;
    use ed25519_dalek::Keypair;
    use near_sdk::test_utils::test_env::{alice, bob};
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::{testing_env, VMContext};

    use crate::util::tests::{acc_claimer, b64_encode, gen_key, mk_claim_sign};

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

    fn class_metadata() -> ClassMetadata {
        ClassMetadata {
            name: "test_1".to_string(),
            symbol: None,
            icon: None,
            reference: None,
            reference_hash: None,
        }
    }

    /// SBT claim ttl in seconds
    const CLAIM_TTL: u64 = 2;

    fn setup(signer: &AccountId, predecessor: &AccountId) -> (VMContext, Contract, Keypair) {
        let ctx = VMContextBuilder::new()
            .signer_account_id(signer.clone())
            .predecessor_account_id(predecessor.clone())
            .attached_deposit(MINT_TOTAL_COST * 5)
            .block_timestamp(start())
            .current_account_id("oracle.near".parse().unwrap())
            .is_view(false)
            .build();

        let keypair = gen_key();
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

        (ctx, ctr, keypair)
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
    fn add_admin() {
        let (_, mut ctr, _) = setup(&acc_claimer(), &acc_admin());
        ctr.add_admin(acc_u1());
        assert_eq!(ctr.get_admins(), vec![acc_admin(), acc_u1()]);

        ctr.remove_admin(acc_admin());
        assert_eq!(ctr.get_admins(), vec![acc_u1()]);
    }

    #[test]
    #[should_panic(
        expected = "Requires attached deposit at least 9000000000000000000000 yoctoNEAR"
    )]
    fn mint_not_enough_storage_deposit() {
        let signer = acc_claimer();
        let (mut ctx, mut ctr, k) = setup(&signer, &acc_u1());

        // fail: not enough storage deposit
        ctx.attached_deposit = MINT_TOTAL_COST - 1;
        testing_env!(ctx);
        let (_, c_str, sig) = mk_claim_sign(start() / SECOND, "0x1a", &k, false);
        let _ = ctr.sbt_mint(c_str, sig, None).expect("must panic");
    }

    #[test]
    #[should_panic(
        expected = "Requires attached deposit at least 18000000000000000000000 yoctoNEAR"
    )]
    fn mint_with_kyc_not_enough_storage_deposit() {
        let signer = acc_claimer();
        let (mut ctx, mut ctr, k) = setup(&signer, &acc_u1());

        // fail: not enough storage deposit
        ctx.attached_deposit = MINT_TOTAL_COST_WITH_KYC - 1;
        testing_env!(ctx);
        let (_, c_str, sig) = mk_claim_sign(start() / SECOND, "0x1a", &k, true);
        let _ = ctr.sbt_mint(c_str, sig, None).expect("must panic");
    }

    #[test]
    fn mint_no_root_account() {
        let signer: AccountId = "user1.near.org".parse().unwrap();
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

        ctx.signer_account_id = acc_bad_implicit();
        testing_env!(ctx.clone());
        assert_bad_request(
            ctr.sbt_mint(c_str.clone(), sig.clone(), None),
            "only root and implicit accounts are allowed to get SBT",
        );

        ctx.signer_account_id = acc_implicit();
        testing_env!(ctx);
        assert_bad_request(
            ctr.sbt_mint(c_str, sig, None),
            "claimer is not the transaction signer",
        );
    }

    #[test]
    fn claim_sig_and_sbt_mint() {
        let signer = "myaccount123.testnet".parse().unwrap();
        let (mut ctx, mut ctr, _) = setup(&signer, &signer);

        // test case based on
        // https://explorer.testnet.near.org/transactions/GobWuBgA9HLsUk4UTtVqrSiyy24P6B2cnywLfeh9mdtv

        ctr.claim_ttl = 100;
        ctx.block_timestamp = 1689675340 * SECOND;
        ctr.authority_pubkey =
            pubkey_from_b64("zqMwV9fTRoBOLXwt1mHxBAF3d0Rh9E9xwSAXR3/KL5E=".to_owned());
        testing_env!(ctx);

        let claim_b64 = "FAAAAG15YWNjb3VudDEyMy50ZXN0bmV0IAAAAGFmZWU5MmYwNzEyMjQ2NGU4MzEzYWFlMjI1Y2U1YTNmSGa2ZAAAAAAA".to_owned();
        let claim_sig_b64 = "38X2TnWgc6moc4zReAJFQ7BjtOUlWZ+i3YQl9gSMOXwnm5gupfHV/YGmGPOek6SSkotT586d4zTTT2U8Qh3GBw==".to_owned();

        let claim_bytes = b64_decode("claim_b64", claim_b64.clone()).unwrap();
        let signature = b64_decode("sig_b64", claim_sig_b64.clone()).unwrap();
        verify_claim(&signature, &claim_bytes, &ctr.authority_pubkey).unwrap();

        let r = ctr.sbt_mint(claim_b64, claim_sig_b64, None);
        match r {
            Ok(_) => (),
            Err(error) => panic!("expected BadRequest, got: {:?}", error),
        }
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
        ctx.signer_account_id = signer;
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
        testing_env!(ctx);
        let resp = ctr.sbt_mint(c_str.clone(), sig.clone(), None);
        assert!(resp.is_ok(), "should accept valid claim");

        // fail: signer already has SBT
        match ctr.sbt_mint(c_str, sig, None) {
            Err(CtrError::DuplicatedID(_)) => (),
            Err(error) => panic!("expected DuplicatedID, got: {:?}", error),
            Ok(_) => panic!("expected DuplicatedID, got: Ok"),
        }
    }

    #[test]
    fn mint_during_elections() {
        let signer = acc_claimer();
        let (mut ctx, mut ctr, k) = setup(&signer, &acc_u1());

        ctx.block_timestamp = (ELECTIONS_START + 1) * 1_000_000;
        ctx.current_account_id = "fractal.i-am-human.near".parse().unwrap();
        testing_env!(ctx.clone());
        let (_, c_str, sig) = mk_claim_sign(start() / SECOND, "0x1a", &k, false);
        let res = ctr.sbt_mint(c_str, sig, None);
        assert!(res.is_err());
        assert_bad_request(res, "IAH SBT cannot be mint during the elections period");

        ctx.block_timestamp = ELECTIONS_END * 1_000_000;
        testing_env!(ctx);
        let (_, c_str, sig) = mk_claim_sign(start() / SECOND, "0x1a", &k, false);
        let res = ctr.sbt_mint(c_str, sig, None);
        assert!(res.is_err());
        assert_bad_request(res, "IAH SBT cannot be mint during the elections period");
    }

    #[test]
    #[should_panic(expected = "not an admin")]
    fn set_class_metadata_not_admin() {
        let (_, mut ctr, _) = setup(&alice(), &alice());
        let _ = ctr.set_class_metadata(1, class_metadata());
    }

    #[test]
    fn set_class_metadata_wrong_class() {
        let (_, mut ctr, _) = setup(&alice(), &acc_admin());
        match ctr.set_class_metadata(3, class_metadata()) {
            Err(CtrError::BadRequest(_)) => (),
            Err(error) => panic!("expected BadRequest, got: {:?}", error),
            Ok(_) => panic!("expected BadRequest, got: Ok"),
        }
    }

    #[test]
    fn set_class_metadata() {
        let (_, mut ctr, _) = setup(&alice(), &acc_admin());
        match ctr.set_class_metadata(1, class_metadata()) {
            Ok(_) => (),
            Err(error) => panic!("expected Ok, got: {:?}", error),
        }
        assert_eq!(ctr.sbt_class_metadata(1).unwrap(), class_metadata());
    }

    #[test]
    #[should_panic(expected = "not an admin")]
    fn admin_mint_not_admin() {
        let (_, mut ctr, _) = setup(&alice(), &alice());
        let _ = ctr.admin_mint(vec![(bob(), 100)], CLASS_FV_SBT, None);
    }

    #[test]
    #[should_panic(expected = "Requires min")]
    fn admin_mint_wrong_deposit() {
        let (mut ctx, mut ctr, _) = setup(&alice(), &acc_admin());
        ctx.attached_deposit = 0;
        testing_env!(ctx);
        let _ = ctr.admin_mint(vec![(bob(), 100), (alice(), 100)], CLASS_FV_SBT, None);
    }

    #[test]
    fn admin_mint() {
        let (_, mut ctr, _) = setup(&alice(), &acc_admin());
        let _ = ctr.admin_mint(vec![(bob(), 100), (alice(), 100)], CLASS_KYC_SBT, None);
        let _ = ctr.admin_mint(vec![(bob(), 100), (alice(), 100)], CLASS_FV_SBT, None);
    }
}
