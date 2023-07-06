use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, LookupMap};
use near_sdk::{env, near_bindgen, require, AccountId, Balance, PanicOnDefault, Promise};

use cost::{MILI_NEAR, MINT_COST, MINT_GAS};
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
    pub minting_authorities: LookupMap<ClassId, Vec<AccountId>>,

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
            minting_authorities: LookupMap::new(StorageKey::MintingAuthority),

            registry,
            metadata: LazyOption::new(StorageKey::ContractMetadata, Some(&metadata)),
            ttl,
        }
    }

    /**********
     * QUERIES
     **********/

    /// Returns minting authorities by class.
    /// If class is not supporte, returns empty list.
    pub fn minting_authorities(&self, class: ClassId) -> Vec<AccountId> {
        self.minting_authorities.get(&class).unwrap_or_default()
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
        self.assert_minter(metadata.class)?;

        let now_ms = env::block_timestamp_ms();
        metadata.expires_at = Some(now_ms + self.ttl);
        metadata.issued_at = Some(now_ms);
        require!(
            1 <= metadata.class && metadata.class <= 3,
            "class ID must be 1, 2 or 3"
        );

        if let Some(memo) = memo {
            env::log_str(&format!("SBT mint memo: {}", memo));
        }

        Ok(ext_registry::ext(self.registry.clone())
            .with_attached_deposit(attached_deposit)
            .with_static_gas(MINT_GAS)
            .sbt_mint(vec![(receiver, vec![metadata])]))
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

    /// admin: authorize `minter` to mint tokens of a `class`.
    /// Must be called by admin, panics otherwise.
    pub fn authorize(
        &mut self,
        class: ClassId,
        minter: AccountId,
        #[allow(unused_variables)] memo: Option<String>,
    ) {
        self.assert_admin();
        let mut ma = self.minting_authorities.get(&class).unwrap_or_default();
        if !ma.contains(&minter) {
            ma.push(minter);
            self.minting_authorities.insert(&class, &ma);
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
        let mut ma = self.minting_authorities.get(&class).unwrap_or_default();
        if let Some(idx) = ma.iter().position(|x| x == &minter) {
            ma.swap_remove(idx);
            self.minting_authorities.insert(&class, &ma);
        }
    }

    /**********
     * INTERNAL
     **********/

    fn assert_admin(&self) {
        require!(self.admin == env::predecessor_account_id(), "not an admin");
    }

    fn assert_minter(&self, class: ClassId) -> Result<(), MintError> {
        if !self
            .minting_authorities(class)
            .contains(&env::predecessor_account_id())
        {
            return Err(MintError::NotMinter);
        }
        return Ok(());
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

    use crate::{required_sbt_mint_deposit, Contract, MintError};

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

    fn setup(predecessor: &AccountId, deposit: Option<Balance>) -> (VMContext, Contract) {
        let mut ctx = VMContextBuilder::new()
            .predecessor_account_id(admin())
            .block_timestamp(START)
            .is_view(false)
            .build();
        ctx.attached_deposit = deposit.unwrap_or(required_sbt_mint_deposit(1));
        testing_env!(ctx.clone());
        let mut ctr = Contract::new(registry(), admin(), contract_metadata(), START);
        ctr.authorize(1, authority(1), None);
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

        let new_cls = 12;
        ctr.authorize(new_cls, authority(2), None);
        match ctr.assert_minter(new_cls) {
            Err(MintError::NotMinter) => (),
            x => panic!("admin should not be a minter of the new class, {:?}", x),
        };

        // authority(1) is a default minter for class 1 in the test setup
        ctx.predecessor_account_id = authority(1);
        testing_env!(ctx.clone());
        ctr.assert_minter(1)?;
        expect_not_authorized(2, &ctr);
        expect_not_authorized(new_cls, &ctr);

        // check authority(2)
        ctx.predecessor_account_id = authority(2);
        testing_env!(ctx.clone());
        ctr.assert_minter(new_cls)?;
        expect_not_authorized(1, &ctr);
        expect_not_authorized(2, &ctr);

        Ok(())
    }

    #[test]
    #[should_panic(expected = "not an admin")]
    fn authorize_only_admin() {
        let (_, mut ctr) = setup(&alice(), None);
        ctr.authorize(1, authority(2), None);
    }

    #[test]
    fn authorize() {
        let (_, mut ctr) = setup(&admin(), None);
        ctr.authorize(1, authority(2), None);
        ctr.authorize(1, authority(2), None);
        ctr.authorize(2, authority(2), None);

        assert_eq!(ctr.minting_authorities(1), vec![authority(1), authority(2)]);
        assert_eq!(ctr.minting_authorities(2), vec![authority(2)]);
        assert_eq!(ctr.minting_authorities(3), vec![]);
    }

    #[test]
    #[should_panic(expected = "not an admin")]
    fn unauthorize_only_admin() {
        let (_, mut ctr) = setup(&alice(), None);
        ctr.unauthorize(1, authority(1), None);
    }

    #[test]
    fn unauthorize() {
        let (_, mut ctr) = setup(&admin(), None);
        ctr.authorize(1, authority(2), None);
        ctr.authorize(1, authority(3), None);
        ctr.authorize(1, authority(4), None);
        ctr.authorize(2, authority(2), None);

        ctr.unauthorize(1, authority(2), None);

        assert_eq!(
            ctr.minting_authorities(1),
            vec![authority(1), authority(4), authority(3)]
        );
        assert_eq!(ctr.minting_authorities(2), vec![authority(2)]);
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
        let (_, mut ctr) = setup(&authority(1), None);

        ctr.sbt_mint(alice(), mk_meteadata(1), None)?;
        match ctr.sbt_mint(alice(), mk_meteadata(2), None) {
            Err(MintError::NotMinter) => (),
            Ok(_) => panic!("expected NotAuthorized, got: Promise"),
            Err(x) => panic!("expected NotAuthorized, got: {:?}", x),
        };

        Ok(())
    }
}
