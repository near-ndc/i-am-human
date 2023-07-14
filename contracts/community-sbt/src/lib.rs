use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, LookupMap};
use near_sdk::{env, near_bindgen, require, AccountId, Balance, PanicOnDefault, Promise};

use cost::{IS_HUMAN_GAS, MILI_NEAR, MINT_COST, MINT_GAS};
use sbt::*;

pub use crate::errors::*;
pub use crate::storage::*;

mod errors;
mod storage;

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
    /// time to live in ms. Overwrites metadata.expire_at.
    pub ttl: u64,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    /// @admin: account authorized to add new minting authority
    /// @ttl: time to live for SBT expire. Must be number in miliseconds.
    #[init]
    pub fn new(
        registry: AccountId,
        admin: AccountId,
        metadata: ContractMetadata,
        ttl: u64,
    ) -> Self {
        require!(ttl > 0, "`ttl` must be bigger than 0");
        Self {
            admin,
            classes: LookupMap::new(StorageKey::MintingAuthority),
            next_class: 1,

            registry,
            metadata: LazyOption::new(StorageKey::ContractMetadata, Some(&metadata)),
            ttl,
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
    /// If `metadata.expires_at` is None then we set it to max: ` now+self.ttl`.
    /// Panics if `metadata.expires_at > now+self.ttl` or when ClassID is not set or not 1.
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
        let requires_iah = self.assert_minter(metadata.class)?;

        let now_ms = env::block_timestamp_ms();
        metadata.expires_at = Some(now_ms + self.ttl);
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
                .sbt_mint(token_spec)
        } else {
            sbt_reg.with_static_gas(MINT_GAS).sbt_mint_iah(token_spec)
        };
        Ok(promise)
    }

    /// sbt_renew will update the expire time of provided tokens.
    /// `ttl` is duration in milliseconds to set expire time: `now+ttl`.
    /// Panics if ttl > self.ttl or ttl < 1h (3'600'000ms) or `tokens` is an empty list.
    pub fn sbt_renew(&mut self, tokens: Vec<TokenId>, ttl: u64, memo: Option<String>) -> Promise {
        self.assert_admin();
        require!(
            3_600_000 <= ttl && ttl <= self.ttl,
            format!(
                "ttl must be bigger than 3'600'000ms smaller or equal than {}ms",
                self.ttl
            )
        );

        require!(!tokens.is_empty(), "tokens must be a non empty list");
        if let Some(memo) = memo {
            env::log_str(&format!("SBT renew memo: {}", memo));
        }

        let expires_at_ms = env::block_timestamp_ms() + ttl;
        ext_registry::ext(self.registry.clone()).sbt_renew(tokens, expires_at_ms)
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
        self.assert_admin();
        if let Some(memo) = memo {
            env::log_str(&format!("SBT revoke memo: {}", memo));
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
        panic!("not implemented");
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
    pub fn change_requires_iah(&mut self, class: ClassId, requires_iah: bool) {
        self.assert_admin();
        let mut c = self.classes.get(&class).expect("class not found");
        if c.requires_iah != requires_iah {
            c.requires_iah = requires_iah;
            self.classes.insert(&class, &c);
        }
    }

    /// Enables a new, unused class and authorizes minter to issue SBTs of that class.
    /// Returns the new class ID.
    pub fn enable_next_class(
        &mut self,
        requires_iah: bool,
        minter: AccountId,
        #[allow(unused_variables)] memo: Option<String>,
    ) -> ClassId {
        self.assert_admin();
        let cls = self.next_class;
        self.next_class += 1;
        self.classes.insert(
            &cls,
            &ClassMinters {
                requires_iah,
                minters: vec![minter],
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

    // returns requires_iah
    fn assert_minter(&self, class: ClassId) -> Result<bool, MintError> {
        match self.class_minter(class) {
            None => Err(MintError::ClassNotEnabled),
            Some(cm) => {
                if cm.minters.contains(&env::predecessor_account_id()) {
                    Ok(cm.requires_iah)
                } else {
                    Err(MintError::NotMinter)
                }
            }
        }
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

    use crate::{required_sbt_mint_deposit, ClassMinters, Contract, MintError};

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

    fn class_minter(requires_iah: bool, minters: Vec<AccountId>) -> ClassMinters {
        ClassMinters {
            requires_iah,
            minters,
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
        let mut ctr = Contract::new(registry(), admin(), contract_metadata(), START);
        ctr.enable_next_class(true, authority(1), None);
        ctx.predecessor_account_id = predecessor.clone();
        testing_env!(ctx.clone());
        return (ctx, ctr);
    }

    #[test]
    fn assert_minter() -> Result<(), MintError> {
        let (mut ctx, mut ctr) = setup(&admin(), None);

        let expect_not_authorized = |cls, ctr: &Contract| match ctr.assert_minter(cls) {
            Err(MintError::NotMinter) => (),
            x => panic!("expected NotAuthorized, got: {:?}", x),
        };

        // admin is not a minter
        expect_not_authorized(1, &ctr);

        let new_cls = ctr.enable_next_class(true, authority(2), None);
        let other_cls = ctr.enable_next_class(true, authority(10), None);
        ctr.authorize(new_cls, authority(3), None);

        match ctr.assert_minter(new_cls) {
            Err(MintError::NotMinter) => (),
            x => panic!("admin should not be a minter of the new class, {:?}", x),
        };

        // authority(1) is a default minter for class 1 in the test setup
        ctx.predecessor_account_id = authority(1);
        testing_env!(ctx.clone());
        ctr.assert_minter(1)?;
        expect_not_authorized(new_cls, &ctr);
        expect_not_authorized(other_cls, &ctr);
        match ctr.assert_minter(1122) {
            Err(MintError::ClassNotEnabled) => (),
            x => panic!("expected ClassNotEnabled, got: {:?}", x),
        };

        // check authority(2)
        ctx.predecessor_account_id = authority(2);
        testing_env!(ctx.clone());
        expect_not_authorized(1, &ctr);
        ctr.assert_minter(new_cls)?;
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
        let cls = ctr.enable_next_class(false, authority(4), None);
        assert_eq!(cls, 2);
        assert_eq!(ctr.next_class, cls + 1);
        let cls = ctr.enable_next_class(false, authority(4), None);
        assert_eq!(cls, 3);
        assert_eq!(ctr.next_class, 4);

        ctr.authorize(1, authority(2), None);
        ctr.authorize(1, authority(2), None);
        ctr.authorize(2, authority(2), None);

        assert_eq!(
            ctr.class_minter(1),
            Some(class_minter(true, vec![authority(1), authority(2)]))
        );
        assert_eq!(
            ctr.class_minter(2),
            Some(class_minter(false, vec![authority(4), authority(2)]))
        );
        assert_eq!(
            ctr.class_minter(3),
            Some(class_minter(false, vec![authority(4)]))
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
        ctr.enable_next_class(false, authority(3), None);

        ctr.authorize(1, authority(2), None);
        ctr.authorize(1, authority(3), None);
        ctr.authorize(1, authority(4), None);
        ctr.authorize(2, authority(2), None);

        ctr.unauthorize(1, authority(2), None);

        assert_eq!(
            ctr.class_minter(1),
            Some(class_minter(
                true,
                vec![authority(1), authority(4), authority(3)]
            ))
        );
        assert_eq!(
            ctr.class_minter(2),
            Some(class_minter(false, vec![authority(3), authority(2)]))
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

        let cls2 = ctr.enable_next_class(true, authority(2), None);

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
}
