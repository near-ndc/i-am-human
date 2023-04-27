use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, UnorderedSet};
use near_sdk::{env, near_bindgen, require, AccountId, Balance, Gas, PanicOnDefault};

use sbt::*;

pub const MINT_COST: Balance = 8_000_000000000000000000; // 0.008 NEAR

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    // contract metadata
    pub metadata: LazyOption<ContractMetadata>,

    /// registry of burned accounts.
    pub registry: AccountId,

    /// max duration (in seconds) a claim is valid for processing
    pub claim_ttl: u64,
    /// SBT ttl until expire in miliseconds (expire=issue_time+sbt_ttl)
    pub sbt_ttl_ms: u64,

    /// used for backend key rotation
    pub admins: UnorderedSet<AccountId>,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(
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
        let mut admins = UnorderedSet::new(b"1");
        admins.insert(&admin);
        Self {
            registry,
            metadata: LazyOption::new(b"m", Some(&metadata)),
            claim_ttl,
            sbt_ttl_ms: 1000 * 3600 * 24 * 365, // 1year in ms
            admins,
        }
    }

    /**********
     * QUERIES
     **********/

    /**********
     * ADMIN
     **********/

    #[payable]
    pub fn sbt_mint(&mut self, receiver: AccountId, memo: Option<String>) {
        self.assert_admin();
        require!(
            env::attached_deposit() == MINT_COST,
            "Requires attached deposit of exactly 0.008 NEAR"
        );

        let now_ms = env::block_timestamp_ms();
        let metadata = TokenMetadata {
            class: 1,
            issued_at: Some(now_ms),
            expires_at: Some(now_ms + self.sbt_ttl_ms),
            reference: None,
            reference_hash: None,
        };

        ext_registry::ext(self.registry.clone())
            .with_attached_deposit(MINT_COST)
            .with_static_gas(Gas::ONE_TERA * 5) // 5 TGas
            .sbt_mint(vec![(receiver, vec![metadata])])
            .then(Self::ext(env::current_account_id()).sbt_mint_callback());

        if let Some(memo) = memo {
            env::log_str(&format!("SBT mint memo: {}", memo));
        }
    }

    #[private]
    #[handle_result]
    pub fn sbt_mint_callback(
        &mut self,
        #[callback_result] last_result: Result<Vec<TokenId>, near_sdk::PromiseError>,
    ) -> Result<Vec<TokenId>, near_sdk::PromiseError> {
        if last_result.is_err() {
            env::panic_str("ERR_CALL_FAILED")
        }
        last_result
    }

    pub fn add_admin(&mut self, account: AccountId, memo: Option<String>) {
        self.assert_admin();
        self.admins.insert(&account);
    }

    /**********
     * INTERNAL
     **********/

    #[inline]
    fn assert_admin(&self) {
        require!(
            self.admins.contains(&env::predecessor_account_id()),
            "not an admin"
        );
    }
}
