use near_sdk::{near_bindgen, AccountId};

use crate::*;
use sbt::*;

fn new_acc(a: &str) -> AccountId {
    AccountId::new_unchecked(a.to_string())
}

fn mock_token_str(token: TokenId, owner: &str) -> Token {
    mock_token(token, new_acc(owner))
}

fn mock_token(token: TokenId, owner: AccountId) -> Token {
    Token {
        token,
        owner,
        metadata: TokenMetadata {
            class: 1,
            issued_at: Some(1680513165),
            expires_at: Some(1685776365),
            reference: Some("https://somelink.com/mydoc".to_owned()),
            reference_hash: Some(
                vec![
                    232, 200, 7, 74, 151, 212, 112, 108, 102, 57, 160, 89, 106, 36, 58, 72, 115,
                    35, 35, 116, 169, 31, 38, 54, 155, 44, 149, 74, 78, 145, 209, 35,
                ]
                .into(),
            ),
        },
    }
}

// Implement the contract structure
#[near_bindgen]
impl SBTRegistry for Contract {
    /**********
     * QUERIES
     **********/

    /// get the information about specific token ID issued by `ctr` SBT contract.
    fn sbt(&self, ctr: AccountId, token: TokenId) -> Option<Token> {
        Some(mock_token_str(token, "alice.near"))
    }

    /// returns total amount of tokens issued by `ctr` SBT contract.
    fn sbt_supply(&self, ctr: AccountId) -> u64 {
        10
    }

    /// returns total amount of tokens of given class minted by this contract
    fn sbt_supply_by_class(&self, ctr: AccountId, class: ClassId) -> u64 {
        2
    }

    /// returns total supply of SBTs for a given owner.
    /// If class is specified, returns only owner supply of the given class -- must be 0 or 1.
    fn sbt_supply_by_owner(
        &self,
        ctr: AccountId,
        account: AccountId,
        class: Option<ClassId>,
    ) -> u64 {
        3
    }

    /// Query sbt tokens issued by a given contract.
    /// If `from_index` is not specified, then `from_index` should be assumed
    /// to be the first valid token id.
    fn sbt_tokens(
        &self,
        ctr: AccountId,
        from_index: Option<u64>,
        limit: Option<u32>,
    ) -> Vec<TokenId> {
        vec![1, 2, 3]
    }

    /// Query SBT tokens by owner
    /// If `from_class` is not specified, then `from_class` should be assumed to be the first
    /// valid class id.
    /// Returns list of pairs: `(Contract address, list of token IDs)`.
    fn sbt_tokens_by_owner(
        &self,
        account: AccountId,
        ctr: Option<AccountId>,
        from_class: Option<u64>,
        limit: Option<u32>,
    ) -> Vec<(AccountId, Vec<TokenId>)> {
        vec![
            (new_acc("alice.near"), vec![1, 2]),
            (new_acc("bob.near"), vec![3]),
        ]
    }

    /// checks if an `account` was banned by the registry.
    fn is_banned(&self, account: AccountId) -> bool {
        self.banlist.contains(&account)
    }

    /*************
     * Transactions
     *************/

    /// Creates a new, unique token and assigns it to the `receiver`.
    /// `token_spec` is a vector of pairs: owner AccountId and TokenMetadata.
    /// Each TokenMetadata must have non zero `class`.
    /// Must be called by an SBT contract.
    /// Must emit `Mint` event.
    /// Must provide enough NEAR to cover registry storage cost.
    #[payable]
    fn sbt_mint(&mut self, token_spec: Vec<(AccountId, TokenMetadata)>) -> Vec<TokenId> {
        let ctr = env::predecessor_account_id();
        self.assert_issuer(&ctr);
        vec![4]
    }

    /// sbt_recover reassigns all tokens from the old owner to a new owner,
    /// and registers `old_owner` to a burned addresses registry.
    /// Must be called by an SBT contract.
    /// Must emit `Recover` event.
    /// Must be called by an operator.
    /// Must provide enough NEAR to cover registry storage cost.
    /// Requires attaching enough tokens to cover the storage growth.
    #[payable]
    fn sbt_recover(&mut self, from: AccountId, to: AccountId) {
        let ctr = env::predecessor_account_id();
        self.assert_issuer(&ctr);
    }

    /// sbt_renew will update the expire time of provided tokens.
    /// `expires_at` is a unix timestamp (in seconds).
    /// Must be called by an SBT contract.
    /// Must emit `Renew` event.
    fn sbt_renew(&mut self, tokens: Vec<TokenId>, expires_at: u64) {
        let ctr = env::predecessor_account_id();
        self.assert_issuer(&ctr);
    }

    /// Revokes SBT, could potentially burn it or update the expire time.
    /// Must be called by an SBT contract.
    /// Must emit one of `Revoke` or `Burn` event.
    /// Returns true if a token is a valid, active SBT. Otherwise returns false.
    fn sbt_revoke(&mut self, token: u64) -> bool {
        let ctr = env::predecessor_account_id();
        self.assert_issuer(&ctr);

        true
    }

    /// Transfers atomically all SBT tokens from one account to another account.
    /// The caller must be an SBT holder and the `to` must not be a banned account.
    /// Must emit `Revoke` event.
    // #[payable]
    fn sbt_soul_transfer(&mut self, to: AccountId) -> bool {
        true
    }
}
