use std::collections::HashMap;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, LookupMap};
use near_sdk::{env, near_bindgen, require, AccountId, PanicOnDefault, Promise};

use cost::{calculate_iah_mint_gas, calculate_mint_gas, mint_deposit};
use sbt::*;

pub use crate::errors::*;
pub use crate::storage::*;

mod errors;
pub mod migrate;
mod storage;

const MIN_TTL: u64 = 86_400_000; // 24 hours in miliseconds
const REGISTRATION_COST: u128 = 1;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// map of classId -> to set of accounts authorized to mint
    pub classes: LookupMap<ClassId, ClassMinters>,
    pub next_class: ClassId,
    /// SBT registry.
    pub registry: AccountId,
    /// contract metadata
    pub metadata: LazyOption<ContractMetadata>,
    pub class_metadata: LookupMap<ClassId, ClassMetadata>,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    /// @admin: account authorized to add new minting authority
    #[init]
    pub fn new(registry: AccountId, metadata: ContractMetadata) -> Self {
        Self {
            classes: LookupMap::new(StorageKey::MintingAuthority),
            next_class: 1,
            registry,
            metadata: LazyOption::new(StorageKey::ContractMetadata, Some(&metadata)),
            class_metadata: LookupMap::new(StorageKey::ClassMetadata),
        }
    }

    /**********
     * QUERIES
     **********/

    /// Returns minting authorities by class. Returns none if the class is not enabled.
    pub fn class_minter(&self, class: ClassId) -> Option<ClassMinters> {
        self.classes.get(&class)
    }

    /// Returns registry address.
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
    ) -> Result<Promise, Error> {
        let token_spec = vec![(receiver, vec![metadata])];
        self.sbt_mint_many(token_spec, memo)
    }

    /// Similar to `sbt_mint`, but allows to mint many tokens at once. See `sbt_mint` doc for
    /// more details.
    /// * `tokens` is list of pairs: token recipient and token metadata to mint for given recipient.
    #[payable]
    #[handle_result]
    pub fn sbt_mint_many(
        &mut self,
        #[allow(unused_mut)] mut token_spec: Vec<(AccountId, Vec<TokenMetadata>)>,
        memo: Option<String>,
    ) -> Result<Promise, Error> {
        let now_ms = env::block_timestamp_ms();
        let mut requires_iah = false;
        let mut class_info_map: HashMap<ClassId, (bool, u64)> = HashMap::new();
        let mut total_len = 0;
        for (_, token_metadatas) in &mut token_spec {
            total_len += token_metadatas.len();
            for m in token_metadatas {
                let (cls_requires_iah, ttl) = match class_info_map.get(&m.class) {
                    Some(ci) => (ci.0, ci.1),
                    None => {
                        let ci = self.class_info_minter(m.class)?;
                        class_info_map.insert(m.class, ci);
                        ci
                    }
                };
                requires_iah = requires_iah || cls_requires_iah;
                m.expires_at = Some(now_ms + ttl);
                m.issued_at = Some(now_ms);
            }
        }

        let required_deposit = mint_deposit(total_len);
        let attached_deposit = env::attached_deposit();
        if attached_deposit < required_deposit {
            return Err(Error::RequiredDeposit(required_deposit));
        }

        if let Some(memo) = memo {
            env::log_str(&format!("SBT mint memo: {}", memo));
        }

        let sbt_reg =
            ext_registry::ext(self.registry.clone()).with_attached_deposit(attached_deposit);
        let promise = if requires_iah {
            let gas = calculate_iah_mint_gas(total_len, token_spec.len());
            sbt_reg.with_static_gas(gas).sbt_mint_iah(token_spec)
        } else {
            sbt_reg
                .with_static_gas(calculate_mint_gas(total_len))
                .sbt_mint(token_spec)
        };

        Ok(promise)
    }

    /// Updates the expire time of provided tokens.
    /// `ttl` is duration in milliseconds to set expire time: `now+ttl`.
    /// Panics if `ttl > self.minters[class].max_ttl` or ttl < `MIN_TTL` or `tokens` is an empty list.
    /// Only minters are allowed to renew the tokens.
    pub fn sbt_renew(&mut self, tokens: Vec<TokenId>, ttl: u64, memo: Option<String>) -> Promise {
        require!(!tokens.is_empty(), "tokens must be a non empty list");
        let caller = env::predecessor_account_id();
        let ctr = env::current_account_id();
        ext_registry::ext(self.registry.clone())
            .sbt_classes(ctr.clone(), tokens.clone())
            .then(Self::ext(ctr).on_sbt_renew_callback(&caller, tokens, ttl, memo))
    }

    /// Callback for sbt_renew. Checks the return value from `sbts` and if any of the tokens
    /// does not exist, the ttl value is invalid or the caller is not a minter panics.
    #[private]
    pub fn on_sbt_renew_callback(
        &self,
        caller: &AccountId,
        tokens: Vec<TokenId>,
        ttl: u64,
        memo: Option<String>,
        #[callback_result] token_classes: Result<Vec<Option<ClassId>>, near_sdk::PromiseError>,
    ) -> Promise {
        let ts = token_classes.expect("error while retrieving tokens data from registry");
        let mut cached_class_info: HashMap<u64, (Vec<AccountId>, u64)> = HashMap::new();
        for token_class in ts {
            let max_ttl: u64;
            let class_id: u64 = token_class.expect("token not found");
            if let Some((cached_minters, cached_ttl)) = cached_class_info.get(&class_id) {
                max_ttl = *cached_ttl;
                self.assert_minter(caller, cached_minters);
            } else {
                max_ttl = self.get_ttl(class_id);
                let minters = self
                    .class_minter(class_id)
                    .expect("class not found")
                    .minters;
                self.assert_minter(caller, &minters);
                cached_class_info.insert(class_id, (minters, max_ttl));
            }
            self.assert_ttl(ttl, max_ttl);
        }
        if let Some(memo) = memo {
            env::log_str(&format!("SBT renew memo: {}", memo));
        }

        let expires_at_ms = env::block_timestamp_ms() + ttl;
        ext_registry::ext(self.registry.clone()).sbt_renew(tokens, expires_at_ms)
    }

    /// Revokes list of tokens. If `burn==true`, the tokens are burned (removed). Otherwise,
    /// the token expire_at is set to now, making the token expired. See `registry.sbt_revoke`
    /// for more details.
    /// Only class minters are allowed to revoke tokens
    pub fn sbt_revoke(
        &mut self,
        tokens: Vec<TokenId>,
        burn: bool,
        memo: Option<String>,
    ) -> Promise {
        require!(!tokens.is_empty(), "tokens must be a non empty list");
        let caller = env::predecessor_account_id();
        let ctr = env::current_account_id();
        ext_registry::ext(self.registry.clone())
            .sbt_classes(ctr.clone(), tokens.clone())
            .then(Self::ext(ctr).on_sbt_revoke_callback(&caller, tokens, burn, memo))
    }

    /// sbt_revoke callback. Checks if all the the tokens can be revoked by the caller
    /// based on the return value from registry.sbts. If not panics with a error message
    #[private]
    pub fn on_sbt_revoke_callback(
        &self,
        caller: &AccountId,
        tokens: Vec<TokenId>,
        burn: bool,
        memo: Option<String>,
        #[callback_result] token_classes: Result<Vec<Option<ClassId>>, near_sdk::PromiseError>,
    ) -> Promise {
        let ts = token_classes.expect("error while retrieving tokens data from registry");
        let mut cached_class_minters: HashMap<u64, Vec<AccountId>> = HashMap::new();
        for token_class in ts {
            let class_id: u64 = token_class.expect("token not found");
            if let Some(cached_minter) = cached_class_minters.get(&class_id) {
                self.assert_minter(caller, cached_minter);
            } else {
                let minters = self
                    .class_minter(class_id)
                    .expect("class not found")
                    .minters;
                self.assert_minter(caller, &minters);
                cached_class_minters.insert(class_id, minters);
            }
        }
        if let Some(memo) = memo {
            env::log_str(&format!("SBT revoke memo: {}", memo));
        }
        ext_registry::ext(self.registry.clone()).sbt_revoke(tokens, burn)
    }

    /// Admin: remove SBT from the given accounts.
    /// Panics if `accounts` is an empty list.
    pub fn revoke_for(
        &mut self,
        accounts: Vec<AccountId>,
        #[allow(unused_variables)] memo: Option<String>,
    ) {
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
    /// Allows admin to change if the specific class requires IAH verification.
    /// Panics if class is not found or not called by a class admin.
    #[handle_result]
    pub fn set_requires_iah(&mut self, class: ClassId, requires_iah: bool) -> Result<(), Error> {
        let mut c = self.class_info_admin(class)?;
        if c.requires_iah != requires_iah {
            c.requires_iah = requires_iah;
            self.classes.insert(&class, &c);
        }
        Ok(())
    }

    /// Allows admin to change Max TTL, expected time duration in miliseconds.
    #[handle_result]
    pub fn set_max_ttl(&mut self, class: ClassId, max_ttl: u64) -> Result<(), Error> {
        let mut c = self.class_info_admin(class)?;
        c.max_ttl = max_ttl;
        self.classes.insert(&class, &c);
        Ok(())
    }

    /// Allows admin to update class metadata.
    /// Panics if class is not enabled.
    #[handle_result]
    pub fn set_sbt_class_metadata(
        &mut self,
        class: ClassId,
        metadata: ClassMetadata,
    ) -> Result<(), Error> {
        self.class_info_admin(class)?;
        self.class_metadata.insert(&class, &metadata);
        Ok(())
    }

    /// Acquires a new, unused class and authorizes minter to issue SBTs of that class.
    /// Caller will become an admin of the class.
    /// Must attach at least REGISTRATION_COST yNEAR to cover storage and bond cost.
    /// Returns the new class ID.
    #[payable]
    pub fn acquire_next_class(
        &mut self,
        requires_iah: bool,
        minters: Vec<AccountId>,
        max_ttl: u64,
        metadata: ClassMetadata,
        #[allow(unused_variables)] memo: Option<String>,
    ) -> ClassId {
        require!(
            MIN_TTL <= max_ttl,
            format!("ttl must be at least {}ms", MIN_TTL)
        );
        require!(
            MIN_TTL <= max_ttl,
            format!("deposit must be at least {}yNEAR", REGISTRATION_COST)
        );
        let cls = self.next_class;
        self.next_class += 1;
        self.classes.insert(
            &cls,
            &ClassMinters {
                admins: vec![env::predecessor_account_id()],
                requires_iah,
                minters,
                max_ttl,
            },
        );
        self.class_metadata.insert(&cls, &metadata);
        cls
    }

    /// Admin: authorize `minter` to mint tokens of a `class`.
    /// Must be called by a class admin, panics otherwise.
    #[handle_result]
    pub fn add_minters(
        &mut self,
        class: ClassId,
        minters: Vec<AccountId>,
        #[allow(unused_variables)] memo: Option<String>,
    ) -> Result<(), Error> {
        let mut c = self.class_info_admin(class)?;
        let mut ok = false;
        for m in minters {
            if !c.minters.contains(&m) {
                c.minters.push(m);
                ok = true;
            }
        }
        if ok {
            self.classes.insert(&class, &c);
        }
        Ok(())
    }

    /// admin: revokes `class` minting for `minter`.
    /// Must be called by a class admin, panics otherwise.
    #[handle_result]
    pub fn remove_minter(
        &mut self,
        class: ClassId,
        minter: AccountId,
        #[allow(unused_variables)] memo: Option<String>,
    ) -> Result<(), Error> {
        let mut c = self.class_info_admin(class)?;
        if let Some(idx) = c.minters.iter().position(|x| x == &minter) {
            c.minters.swap_remove(idx);
            self.classes.insert(&class, &c);
        }
        Ok(())
    }

    /// admin: updates this SBT ContractMetadata
    /// Must be called by a contract admin, panics otherwise.
    #[handle_result]
    #[private]
    pub fn update_metadata(&mut self, metadata: ContractMetadata) {
        self.metadata.replace(&metadata);
    }

    /**********
     * INTERNAL
     **********/

    /// Returns error if class is not found or not called by an admin.
    fn class_info_admin(&self, class: ClassId) -> Result<ClassMinters, Error> {
        match self.class_minter(class) {
            None => Err(Error::ClassNotFound),
            Some(cm) => {
                if cm.admins.contains(&env::predecessor_account_id()) {
                    Ok(cm)
                } else {
                    Err(Error::NotAdmin)
                }
            }
        }
    }

    /// Returns (requires_iah, max_ttl).
    /// Returns error if class is not found or not called by a minter or an admin.
    fn class_info_minter(&self, class: ClassId) -> Result<(bool, u64), Error> {
        match self.class_minter(class) {
            None => Err(Error::ClassNotFound),
            Some(cm) => {
                let a = &env::predecessor_account_id();
                if cm.minters.contains(a) || cm.admins.contains(a) {
                    Ok((cm.requires_iah, cm.max_ttl))
                } else {
                    Err(Error::NotMinter)
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
impl SBTIssuer for Contract {
    fn sbt_metadata(&self) -> ContractMetadata {
        self.metadata.get().unwrap()
    }

    /// Returns `ClassMetadata` by class. Returns none if the class is not enabled.
    fn sbt_class_metadata(&self, class: ClassId) -> Option<ClassMetadata> {
        self.class_metadata.get(&class)
    }
}

#[cfg(test)]
mod tests {
    use cost::mint_deposit;
    use near_sdk::{
        test_utils::{
            test_env::{alice, bob, carol},
            VMContextBuilder,
        },
        testing_env, AccountId, Balance, VMContext,
    };
    use sbt::{ClassId, ClassMetadata, ContractMetadata, SBTIssuer, TokenMetadata};

    use crate::{ClassMinters, Contract, Error, MIN_TTL};

    const START: u64 = 10;

    fn registry() -> AccountId {
        AccountId::new_unchecked("registry.near".to_string())
    }

    fn admin() -> AccountId {
        AccountId::new_unchecked("sbt.near".to_string())
    }

    fn auth(i: u8) -> AccountId {
        AccountId::new_unchecked(format!("authority{}.near", i))
    }

    fn contract_metadata() -> ContractMetadata {
        ContractMetadata {
            spec: "community-sbt-0.0.1".to_string(),
            name: "community-sbt".to_string(),
            symbol: "COMMUNITY_SBT".to_string(),
            icon: None,
            base_uri: None,
            reference: None,
            reference_hash: None,
        }
    }

    fn class_minter(requires_iah: bool, minters: Vec<AccountId>, max_ttl: u64) -> ClassMinters {
        ClassMinters {
            admins: vec![admin()],
            requires_iah,
            minters,
            max_ttl,
        }
    }

    fn class_metadata(c: ClassId) -> ClassMetadata {
        ClassMetadata {
            name: format!("cls-{}", c),
            symbol: None,
            icon: None,
            reference: None,
            reference_hash: None,
        }
    }

    fn setup(predecessor: &AccountId, deposit: Option<Balance>) -> (VMContext, Contract) {
        let mut ctx = VMContextBuilder::new()
            .predecessor_account_id(admin())
            .block_timestamp(START)
            .is_view(false)
            .build();
        ctx.attached_deposit = deposit.unwrap_or(mint_deposit(1));
        testing_env!(ctx.clone());
        let mut ctr = Contract::new(registry(), contract_metadata());
        let c = ctr.acquire_next_class(true, vec![auth(1)], MIN_TTL, class_metadata(1), None);
        assert_eq!(c, 1);
        ctx.predecessor_account_id = predecessor.clone();
        testing_env!(ctx.clone());
        (ctx, ctr)
    }

    #[test]
    fn class_info() -> Result<(), Error> {
        let (mut ctx, mut ctr) = setup(&admin(), None);

        let expect_not_authorized = |cls, ctr: &Contract| match ctr.class_info_minter(cls) {
            Err(Error::NotMinter) => (),
            x => panic!("expected NotAuthorized for cls {}, got: {:?}", cls, x),
        };

        ctr.class_info_minter(1)?; // admin can mint

        let cls2 = ctr.acquire_next_class(true, vec![auth(2)], MIN_TTL, class_metadata(2), None);
        let cls3 = ctr.acquire_next_class(true, vec![auth(10)], MIN_TTL, class_metadata(3), None);
        ctr.add_minters(cls2, vec![auth(3)], None)?;

        ctr.class_info_minter(cls2)?; // admin can mint

        // authority(1) is a default minter for class 1 in the test setup
        ctx.predecessor_account_id = auth(1);
        testing_env!(ctx.clone());
        ctr.class_info_minter(1)?;
        expect_not_authorized(cls2, &ctr);
        expect_not_authorized(cls3, &ctr);
        match ctr.class_info_minter(1122) {
            Err(Error::ClassNotFound) => (),
            x => panic!("expected ClassNotEnabled, got: {:?}", x),
        };

        // check authority(2)
        ctx.predecessor_account_id = auth(2);
        testing_env!(ctx);
        expect_not_authorized(1, &ctr);
        ctr.class_info_minter(cls2)?;
        expect_not_authorized(cls3, &ctr);

        Ok(())
    }

    #[test]
    fn add_minter() -> Result<(), Error> {
        let (mut ctx, mut ctr) = setup(&admin(), None);

        // class not found
        matches!(
            ctr.add_minters(2, vec![auth(2)], None),
            Err(Error::ClassNotFound)
        );

        assert_eq!(ctr.sbt_class_metadata(1), Some(class_metadata(1)));
        assert_eq!(ctr.sbt_class_metadata(0), None);
        assert_eq!(ctr.sbt_class_metadata(2), None);
        assert_eq!(ctr.sbt_class_metadata(322), None);

        assert_eq!(ctr.class_minter(0), None);
        assert_eq!(ctr.class_minter(2), None);
        assert_eq!(ctr.class_minter(2415), None);

        let cls = ctr.acquire_next_class(false, vec![auth(4)], MIN_TTL, class_metadata(2), None);
        assert_eq!(cls, 2);
        assert_eq!(ctr.next_class, cls + 1);
        let cls = ctr.acquire_next_class(false, vec![auth(4)], MIN_TTL, class_metadata(3), None);
        assert_eq!(cls, 3);
        assert_eq!(ctr.next_class, 4);

        ctr.add_minters(1, vec![auth(2)], None)?;
        ctr.add_minters(2, vec![auth(2), auth(2)], None)?;

        // verify class minters
        assert_eq!(
            ctr.class_minter(1),
            Some(class_minter(true, vec![auth(1), auth(2)], MIN_TTL))
        );
        assert_eq!(
            ctr.class_minter(2),
            Some(class_minter(false, vec![auth(4), auth(2)], MIN_TTL))
        );
        assert_eq!(
            ctr.class_minter(3),
            Some(class_minter(false, vec![auth(4)], MIN_TTL))
        );
        assert_eq!(ctr.class_minter(4), None);

        // verify class metadata
        assert_eq!(ctr.sbt_class_metadata(1), Some(class_metadata(1)));
        assert_eq!(ctr.sbt_class_metadata(2), Some(class_metadata(2)));
        assert_eq!(ctr.sbt_class_metadata(3), Some(class_metadata(3)));
        assert_eq!(ctr.class_minter(0), None);
        assert_eq!(ctr.class_minter(4), None);
        assert_eq!(ctr.class_minter(5), None);
        assert_eq!(ctr.class_minter(2412), None);

        // not an admin
        ctx.predecessor_account_id = alice();
        testing_env!(ctx.clone());
        matches!(
            ctr.add_minters(1, vec![auth(200)], None),
            Err(Error::NotAdmin)
        );

        ctx.predecessor_account_id = auth(1);
        testing_env!(ctx.clone());
        matches!(
            ctr.add_minters(1, vec![auth(200)], None),
            Err(Error::NotAdmin)
        );

        Ok(())
    }

    #[test]
    fn remove_minter() -> Result<(), Error> {
        let (mut ctx, mut ctr) = setup(&admin(), None);

        matches!(
            ctr.remove_minter(2, auth(1), None),
            Err(Error::ClassNotFound)
        );

        ctr.acquire_next_class(false, vec![auth(3)], MIN_TTL, class_metadata(2), None);

        ctr.add_minters(1, vec![auth(2), auth(3), auth(4)], None)?;
        ctr.add_minters(2, vec![auth(2)], None)?;

        ctr.remove_minter(1, auth(2), None)?;

        assert_eq!(
            ctr.class_minter(1),
            Some(class_minter(true, vec![auth(1), auth(4), auth(3)], MIN_TTL))
        );
        assert_eq!(
            ctr.class_minter(2),
            Some(class_minter(false, vec![auth(3), auth(2)], MIN_TTL))
        );

        ctx.predecessor_account_id = alice();
        testing_env!(ctx.clone());
        matches!(ctr.remove_minter(1, auth(1), None), Err(Error::NotAdmin));

        Ok(())
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
    fn mint() -> Result<(), Error> {
        let (mut ctx, mut ctr) = setup(&admin(), None);

        let cls2 = ctr.acquire_next_class(true, vec![auth(2)], MIN_TTL, class_metadata(2), None);

        ctx.predecessor_account_id = auth(1);
        testing_env!(ctx);

        ctr.sbt_mint(alice(), mk_meteadata(1), None)?;
        match ctr.sbt_mint(alice(), mk_meteadata(cls2), None) {
            Err(Error::NotMinter) => (),
            Ok(_) => panic!("expected NotAuthorized, got: Ok"),
            Err(x) => panic!("expected NotAuthorized, got: {:?}", x),
        };

        match ctr.sbt_mint(alice(), mk_meteadata(1122), None) {
            Err(Error::ClassNotFound) => (),
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

    #[test]
    #[should_panic(expected = "caller must be a minter")]
    fn assert_minter() {
        let (_, ctr) = setup(&admin(), None);
        ctr.assert_minter(&alice(), &vec![bob()]);
    }

    #[test]
    fn mint_many() -> Result<(), Error> {
        let (mut ctx, mut ctr) = setup(&admin(), None);

        let cls2 = ctr.acquire_next_class(true, vec![auth(1)], MIN_TTL, class_metadata(2), None);
        let cls3 = ctr.acquire_next_class(true, vec![auth(2)], MIN_TTL, class_metadata(3), None);

        ctx.predecessor_account_id = auth(1);
        testing_env!(ctx.clone());

        match ctr.sbt_mint_many(
            vec![
                (alice(), vec![mk_meteadata(1), mk_meteadata(cls2)]),
                (bob(), vec![mk_meteadata(1), mk_meteadata(cls2)]),
            ],
            None,
        ) {
            Err(Error::RequiredDeposit(36000000000000000000000)) => (),
            Ok(_) => panic!("expected RequiredDeposit, got: Ok"),
            Err(x) => panic!("expected RequiredDeposit, got: {:?}", x),
        };

        match ctr.sbt_mint_many(
            vec![
                (alice(), vec![mk_meteadata(1), mk_meteadata(cls2)]),
                (bob(), vec![mk_meteadata(1), mk_meteadata(cls3)]),
            ],
            None,
        ) {
            Err(Error::NotMinter) => (),
            Ok(_) => panic!("expected NotAuthorized, got: Ok"),
            Err(x) => panic!("expected NotAuthorized, got: {:?}", x),
        };

        match ctr.sbt_mint_many(
            vec![
                (alice(), vec![mk_meteadata(1), mk_meteadata(cls2)]),
                (bob(), vec![mk_meteadata(1122), mk_meteadata(cls2)]),
            ],
            None,
        ) {
            Err(Error::ClassNotFound) => (),
            Ok(_) => panic!("expected ClassNotEnabled, got: Ok"),
            Err(x) => panic!("expected NotAuthorized, got: {:?}", x),
        };

        ctx.attached_deposit = 19000000000000000000000;
        testing_env!(ctx.clone());
        ctr.sbt_mint_many(
            vec![(alice(), vec![mk_meteadata(1), mk_meteadata(cls2)])],
            None,
        )?;

        // deposit increases because we are minting more tokens
        ctx.attached_deposit = 37000000000000000000000;
        testing_env!(ctx);
        ctr.sbt_mint_many(
            vec![
                (bob(), vec![mk_meteadata(1), mk_meteadata(cls2)]),
                (carol(), vec![mk_meteadata(1), mk_meteadata(cls2)]),
            ],
            None,
        )?;

        Ok(())
    }

    #[test]
    fn assert_admin() {
        let (mut ctx, ctr) = setup(&admin(), None);

        ctx.predecessor_account_id = alice();
        testing_env!(ctx);
        matches!(ctr.class_info_admin(1), Err(Error::NotAdmin));
    }
}
