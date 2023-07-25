use std::collections::HashMap;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, LookupMap};
use near_sdk::{env, near_bindgen, require, AccountId, Balance, PanicOnDefault, Promise};

use cost::{IS_HUMAN_GAS, MILI_NEAR, MINT_COST, MINT_GAS};
use sbt::*;

pub use crate::errors::*;
pub use crate::storage::*;

mod errors;
pub mod migrate;
mod storage;

const MIN_TTL: u64 = 86_400_000; // 24 hours in miliseconds

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// Account authorized to add new minting authority
    pub admin: AccountId,
    /// map of classId -> to set of accounts authorized to mint
    pub classes: LookupMap<ClassId, ClassMinters>,
    pub next_class: ClassId,
    /// SBT registry.
    pub registry: AccountId,
    /// contract metadata
    pub metadata: LazyOption<ContractMetadata>,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    /// @admin: account authorized to add new minting authority
    #[init]
    pub fn new(registry: AccountId, admin: AccountId, metadata: ContractMetadata) -> Self {
        Self {
            admin,
            classes: LookupMap::new(StorageKey::MintingAuthority),
            next_class: 1,
            registry,
            metadata: LazyOption::new(StorageKey::ContractMetadata, Some(&metadata)),
        }
    }

    /**********
     * QUERIES
     **********/

    /// Returns minting authorities by class.
    /// If the `class` is enabled, returns class minter, otherwise returns None.
    pub fn class_minter(&self, class: ClassId) -> Option<ClassMinters> {
        self.classes.get(&class)
    }

    /// Returns registry address
    pub fn registry(&self) -> AccountId {
        self.registry.clone()
    }

    /**********
     * Transactions
     **********/

    /// Mints a new SBT for the given receiver.
    /// If `metadata.expires_at` is None then we set it to max: ` now+max_ttl`.
    /// Panics if `metadata.expires_at > now+max_ttl` or when ClassID is not set or not 1.
    #[payable]
    #[handle_result]
    pub fn sbt_mint(
        &mut self,
        receiver: AccountId,
        #[allow(unused_mut)] mut metadata: TokenMetadata,
        memo: Option<String>,
    ) -> Result<Promise, MintError> {
        let required_deposit = required_sbt_mint_deposit(1);
        let attached_deposit = env::attached_deposit();
        if attached_deposit < required_deposit {
            return Err(MintError::RequiredDeposit(required_deposit));
        }
        let (requires_iah, ttl) = self.class_info(metadata.class)?;
        let now_ms = env::block_timestamp_ms();
        metadata.expires_at = Some(now_ms + ttl);
        metadata.issued_at = Some(now_ms);
        if let Some(memo) = memo {
            env::log_str(&format!("SBT mint memo: {}", memo));
        }

        let token_spec = vec![(receiver, vec![metadata])];
        let sbt_reg =
            ext_registry::ext(self.registry.clone()).with_attached_deposit(attached_deposit);
        let promise = if requires_iah {
            sbt_reg
                .with_static_gas(MINT_GAS + IS_HUMAN_GAS)
                .sbt_mint_iah(token_spec)
        } else {
            sbt_reg.with_static_gas(MINT_GAS).sbt_mint(token_spec)
        };
        Ok(promise)
    }

    /// sbt_renew will update the expire time of provided tokens.
    /// `ttl` is duration in milliseconds to set expire time: `now+ttl`.
    /// Panics if `ttl > self.minters[class].max_ttl` or ttl < `MIN_TTL` or `tokens` is an empty list.
    pub fn sbt_renew(&mut self, tokens: Vec<TokenId>, ttl: u64, memo: Option<String>) -> Promise {
        ext_registry::ext(self.registry.clone())
            .sbts(env::current_account_id(), tokens.clone())
            .then(Self::ext(env::current_account_id()).on_sbt_renew_callback(ttl));
        require!(!tokens.is_empty(), "tokens must be a non empty list");
        if let Some(memo) = memo {
            env::log_str(&format!("SBT renew memo: {}", memo));
        }

        let expires_at_ms = env::block_timestamp_ms() + ttl;
        ext_registry::ext(self.registry.clone()).sbt_renew(tokens, expires_at_ms)
    }

    /// callback for sbt_renew. Checks the return value from `sbts` and if any of the tokens
    /// does not exist or the ttl value is invalid panics.
    #[private]
    pub fn on_sbt_renew_callback(
        &self,
        ttl: u64,
        #[callback_result] token_data: Result<Vec<Option<Token>>, near_sdk::PromiseError>,
    ) {
        let ts = token_data.expect("error while retrieving tokens data from registry");
        let mut cached_ttl: HashMap<u64, u64> = HashMap::new();
        for token in ts {
            let max_ttl: u64;
            let class_id: u64 = token.expect("token not found").metadata.class;
            if let Some(cached_ttl) = cached_ttl.get(&class_id) {
                max_ttl = *cached_ttl;
            } else {
                max_ttl = self.get_ttl(class_id);
                cached_ttl.insert(class_id, max_ttl);
            }
            self.assert_ttl(ttl, max_ttl);
        }
    }

    /// Revokes list of tokens. If `burn==true`, the tokens are burned (removed). Otherwise,
    /// the token expire_at is set to now, making the token expired. See `registry.sbt_revoke`
    /// for more details.
    pub fn sbt_revoke(
        &mut self,
        tokens: Vec<TokenId>,
        burn: bool,
        memo: Option<String>,
    ) -> Promise {
        // assert is either admin or a token minter
        let caller = env::predecessor_account_id();
        ext_registry::ext(self.registry.clone())
            .sbts(env::current_account_id(), tokens.clone())
            .then(
                Self::ext(env::current_account_id()).on_sbt_revoke_callback(&caller, tokens, burn),
            )
        // if let Some(memo) = memo {
        //     env::log_str(&format!("SBT revoke memo: {}", memo));
        // }
    }

    #[private]
    pub fn on_sbt_revoke_callback(
        &self,
        caller: &AccountId,
        tokens: Vec<TokenId>,
        burn: bool,
        #[callback_result] token_data: Result<Vec<Option<Token>>, near_sdk::PromiseError>,
    ) -> Promise {
        let ts = token_data.expect("error while retrieving tokens data from registry");
        let mut cached_class_minters: HashMap<u64, Vec<AccountId>> = HashMap::new();
        let mut minters;
        for token in ts {
            let class_id: u64 = token.expect("token not found").metadata.class;
            if let Some(cached_minter) = cached_class_minters.get(&class_id) {
                minters = cached_minter.to_vec();
            } else {
                minters = self
                    .class_minter(class_id)
                    .expect("class not found")
                    .minters;
                cached_class_minters.insert(class_id, minters.clone());
            }
            self.assert_minter(caller, &minters);
        }
        ext_registry::ext(self.registry.clone()).sbt_revoke(tokens, burn)
    }

    /// admin: remove SBT from the given accounts.
    /// Panics if `accounts` is an empty list.
    pub fn revoke_for(
        &mut self,
        accounts: Vec<AccountId>,
        #[allow(unused_variables)] memo: Option<String>,
    ) {
        self.assert_admin();
        require!(!accounts.is_empty(), "accounts must be a non empty list");
        env::panic_str("not implemented");
        // todo: requires registry update.
        // let mut tokens = Vec::with_capacity(accounts.len());
        // for a in accounts {
        //     tokens.push(t);
        // }
        // if !tokens.is_empty() {
        //     SbtTokensEvent { tokens, memo }.emit_revoke();
        // }
    }

    /**********
     * Admin
     **********/

    /// allows admin to change if the specific class requires IAH verification.
    /// Panics if class is not found.
    pub fn set_requires_iah(&mut self, class: ClassId, requires_iah: bool) {
        self.assert_admin();
        let mut c = self.classes.get(&class).expect("class not found");
        if c.requires_iah != requires_iah {
            c.requires_iah = requires_iah;
            self.classes.insert(&class, &c);
        }
    }

    /// allows admin to change Max TTL, expected time duration in miliseconds.
    pub fn set_max_ttl(&mut self, class: ClassId, max_ttl: u64) {
        self.assert_admin();
        let mut cm = self.classes.get(&class).expect("class not found");
        cm.max_ttl = max_ttl;
        self.classes.insert(&class, &cm);
    }

    /// Enables a new, unused class and authorizes minter to issue SBTs of that class.
    /// Returns the new class ID.
    pub fn enable_next_class(
        &mut self,
        requires_iah: bool,
        minter: AccountId,
        max_ttl: u64,
        #[allow(unused_variables)] memo: Option<String>,
    ) -> ClassId {
        self.assert_admin();
        require!(
            MIN_TTL <= max_ttl,
            format!("ttl must be at least {}ms", MIN_TTL)
        );
        let cls = self.next_class;
        self.next_class += 1;
        self.classes.insert(
            &cls,
            &ClassMinters {
                requires_iah,
                minters: vec![minter],
                max_ttl,
            },
        );
        cls
    }

    /// admin: authorize `minter` to mint tokens of a `class`.
    /// Must be called by admin, panics otherwise.
    pub fn authorize(
        &mut self,
        class: ClassId,
        minter: AccountId,
        #[allow(unused_variables)] memo: Option<String>,
    ) {
        self.assert_admin();
        let mut c = self.classes.get(&class).expect("class not found");
        if !c.minters.contains(&minter) {
            c.minters.push(minter);
            self.classes.insert(&class, &c);
        }
    }

    /// admin: revokes `class` minting for `minter`.
    /// Must be called by admin, panics otherwise.
    pub fn unauthorize(
        &mut self,
        class: ClassId,
        minter: AccountId,
        #[allow(unused_variables)] memo: Option<String>,
    ) {
        self.assert_admin();
        let mut c = self.classes.get(&class).expect("class not found");
        if let Some(idx) = c.minters.iter().position(|x| x == &minter) {
            c.minters.swap_remove(idx);
            self.classes.insert(&class, &c);
        }
    }

    pub fn change_admin(&mut self, new_admin: AccountId) {
        self.assert_admin();
        self.admin = new_admin;
    }

    /// admin: authorize `minter` to mint tokens of a `class`.
    /// Must be called by admin, panics otherwise.
    pub fn update_metadata(&mut self, metadata: ContractMetadata) {
        self.assert_admin();
        self.metadata.replace(&metadata);
    }

    /**********
     * INTERNAL
     **********/

    fn assert_admin(&self) {
        require!(self.admin == env::predecessor_account_id(), "not an admin");
    }

    /// Returns (requires_iah, max_ttl).
    /// Returns error if class is not found or called by not minter.
    fn class_info(&self, class: ClassId) -> Result<(bool, u64), MintError> {
        match self.class_minter(class) {
            None => Err(MintError::ClassNotEnabled),
            Some(cm) => {
                if cm.minters.contains(&env::predecessor_account_id()) {
                    Ok((cm.requires_iah, cm.max_ttl))
                } else {
                    Err(MintError::NotMinter)
                }
            }
        }
    }

    /// returns ttl for a given token class
    fn get_ttl(&self, class: ClassId) -> u64 {
        match self.class_minter(class) {
            None => panic!("class not found"),
            Some(cm) => cm.max_ttl,
        }
    }

    fn assert_ttl(&self, ttl: u64, max_ttl: u64) {
        require!(
            ttl <= max_ttl,
            format!("ttl must be smaller or equal than {}ms", max_ttl)
        );
    }

    fn assert_minter(&self, caller: &AccountId, minters: &Vec<AccountId>) {
        require!(minters.contains(caller), "caller must be a minter");
    }
}

#[near_bindgen]
impl SBTContract for Contract {
    fn sbt_metadata(&self) -> ContractMetadata {
        self.metadata.get().unwrap()
    }
}

#[inline]
pub fn required_sbt_mint_deposit(num_tokens: usize) -> Balance {
    (num_tokens as u128) * MINT_COST + MILI_NEAR
}

#[cfg(test)]
mod tests {
    use near_sdk::{test_utils::VMContextBuilder, testing_env, AccountId, Balance, VMContext};
    use sbt::{ClassId, ContractMetadata, TokenMetadata};

    use crate::{required_sbt_mint_deposit, ClassMinters, Contract, MintError, MIN_TTL};

    const START: u64 = 10;

    fn alice() -> AccountId {
        AccountId::new_unchecked("alice.near".to_string())
    }

    fn registry() -> AccountId {
        AccountId::new_unchecked("registry.near".to_string())
    }

    fn admin() -> AccountId {
        AccountId::new_unchecked("sbt.near".to_string())
    }

    fn authority(i: u8) -> AccountId {
        AccountId::new_unchecked(format!("authority{}.near", i))
    }

    fn contract_metadata() -> ContractMetadata {
        return ContractMetadata {
            spec: "community-sbt-0.0.1".to_string(),
            name: "community-sbt".to_string(),
            symbol: "COMMUNITY_SBT".to_string(),
            icon: None,
            base_uri: None,
            reference: None,
            reference_hash: None,
        };
    }

    fn class_minter(requires_iah: bool, minters: Vec<AccountId>, max_ttl: u64) -> ClassMinters {
        ClassMinters {
            requires_iah,
            minters,
            max_ttl,
        }
    }

    fn setup(predecessor: &AccountId, deposit: Option<Balance>) -> (VMContext, Contract) {
        let mut ctx = VMContextBuilder::new()
            .predecessor_account_id(admin())
            .block_timestamp(START)
            .is_view(false)
            .build();
        ctx.attached_deposit = deposit.unwrap_or(required_sbt_mint_deposit(1));
        testing_env!(ctx.clone());
        let mut ctr = Contract::new(registry(), admin(), contract_metadata());
        ctr.enable_next_class(true, authority(1), MIN_TTL, None);
        ctx.predecessor_account_id = predecessor.clone();
        testing_env!(ctx.clone());
        return (ctx, ctr);
    }

    #[test]
    fn class_info() -> Result<(), MintError> {
        let (mut ctx, mut ctr) = setup(&admin(), None);

        let expect_not_authorized = |cls, ctr: &Contract| match ctr.class_info(cls) {
            Err(MintError::NotMinter) => (),
            x => panic!("expected NotAuthorized, got: {:?}", x),
        };

        // admin is not a minter
        expect_not_authorized(1, &ctr);

        let new_cls = ctr.enable_next_class(true, authority(2), MIN_TTL, None);
        let other_cls = ctr.enable_next_class(true, authority(10), MIN_TTL, None);
        ctr.authorize(new_cls, authority(3), None);

        match ctr.class_info(new_cls) {
            Err(MintError::NotMinter) => (),
            x => panic!("admin should not be a minter of the new class, {:?}", x),
        };

        // authority(1) is a default minter for class 1 in the test setup
        ctx.predecessor_account_id = authority(1);
        testing_env!(ctx.clone());
        ctr.class_info(1)?;
        expect_not_authorized(new_cls, &ctr);
        expect_not_authorized(other_cls, &ctr);
        match ctr.class_info(1122) {
            Err(MintError::ClassNotEnabled) => (),
            x => panic!("expected ClassNotEnabled, got: {:?}", x),
        };

        // check authority(2)
        ctx.predecessor_account_id = authority(2);
        testing_env!(ctx.clone());
        expect_not_authorized(1, &ctr);
        ctr.class_info(new_cls)?;
        expect_not_authorized(other_cls, &ctr);

        Ok(())
    }

    #[test]
    #[should_panic(expected = "not an admin")]
    fn authorize_only_admin() {
        let (_, mut ctr) = setup(&alice(), None);
        ctr.authorize(1, authority(2), None);
    }

    #[test]
    #[should_panic(expected = "class not found")]
    fn authorize_class_not_found() {
        let (_, mut ctr) = setup(&admin(), None);
        ctr.authorize(2, authority(2), None);
    }

    #[test]
    fn authorize() {
        let (_, mut ctr) = setup(&admin(), None);
        let cls = ctr.enable_next_class(false, authority(4), MIN_TTL, None);
        assert_eq!(cls, 2);
        assert_eq!(ctr.next_class, cls + 1);
        let cls = ctr.enable_next_class(false, authority(4), MIN_TTL, None);
        assert_eq!(cls, 3);
        assert_eq!(ctr.next_class, 4);

        ctr.authorize(1, authority(2), None);
        ctr.authorize(1, authority(2), None);
        ctr.authorize(2, authority(2), None);

        assert_eq!(
            ctr.class_minter(1),
            Some(class_minter(
                true,
                vec![authority(1), authority(2)],
                MIN_TTL
            ))
        );
        assert_eq!(
            ctr.class_minter(2),
            Some(class_minter(
                false,
                vec![authority(4), authority(2)],
                MIN_TTL
            ))
        );
        assert_eq!(
            ctr.class_minter(3),
            Some(class_minter(false, vec![authority(4)], MIN_TTL))
        );
        assert_eq!(ctr.class_minter(4), None);
    }

    #[test]
    #[should_panic(expected = "not an admin")]
    fn unauthorize_only_admin() {
        let (_, mut ctr) = setup(&alice(), None);
        ctr.unauthorize(1, authority(1), None);
    }

    #[test]
    #[should_panic(expected = "class not found")]
    fn unauthorize_class_not_found() {
        let (_, mut ctr) = setup(&admin(), None);
        ctr.unauthorize(2, authority(1), None);
    }

    #[test]
    fn unauthorize() {
        let (_, mut ctr) = setup(&admin(), None);
        ctr.enable_next_class(false, authority(3), MIN_TTL, None);

        ctr.authorize(1, authority(2), None);
        ctr.authorize(1, authority(3), None);
        ctr.authorize(1, authority(4), None);
        ctr.authorize(2, authority(2), None);

        ctr.unauthorize(1, authority(2), None);

        assert_eq!(
            ctr.class_minter(1),
            Some(class_minter(
                true,
                vec![authority(1), authority(4), authority(3)],
                MIN_TTL
            ))
        );
        assert_eq!(
            ctr.class_minter(2),
            Some(class_minter(
                false,
                vec![authority(3), authority(2)],
                MIN_TTL
            ))
        );
    }

    fn mk_meteadata(class: ClassId) -> TokenMetadata {
        TokenMetadata {
            class,
            issued_at: None,
            expires_at: None,
            reference: None,
            reference_hash: None,
        }
    }

    #[test]
    fn mint() -> Result<(), MintError> {
        let (mut ctx, mut ctr) = setup(&admin(), None);

        let cls2 = ctr.enable_next_class(true, authority(2), MIN_TTL, None);

        ctx.predecessor_account_id = authority(1);
        testing_env!(ctx.clone());

        ctr.sbt_mint(alice(), mk_meteadata(1), None)?;
        match ctr.sbt_mint(alice(), mk_meteadata(cls2), None) {
            Err(MintError::NotMinter) => (),
            Ok(_) => panic!("expected NotAuthorized, got: Promise"),
            Err(x) => panic!("expected NotAuthorized, got: {:?}", x),
        };

        match ctr.sbt_mint(alice(), mk_meteadata(1122), None) {
            Err(MintError::ClassNotEnabled) => (),
            Ok(_) => panic!("expected ClassNotEnabled, got: Ok"),
            Err(x) => panic!("expected NotAuthorized, got: {:?}", x),
        };

        Ok(())
    }
    #[test]
    #[should_panic(expected = "ttl must be smaller or equal than 1ms")]
    fn assert_ttl() {
        let (_, ctr) = setup(&admin(), None);
        ctr.assert_ttl(10, 1);
    }
}
