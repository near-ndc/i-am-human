use crate::*;

#[near_bindgen]
impl Contract {
    //
    // TESTING
    // list of functions used in backstage for testing
    //

    pub fn admin_add_minter(&mut self, minter: AccountId) {
        self.assert_authority();
        self.admins.push(minter);
    }

    fn assert_admins(&self) {
        if !self.admins.is_empty() {
            require!(
                self.admins.contains(&env::predecessor_account_id()),
                "only admins are allowed to mint tokens"
            );
        }
    }

    /// asserts that the account name contains "test" (eg *.testnet, mainnet-testing.* ...).
    fn assert_testing(&self) {
        require!(
            env::current_account_id().as_str().contains("test"),
            "must be testnet"
        );
    }

    /// returns false if the `issuer` contract was already registered.
    pub fn testing_add_sbt_issuer(&mut self, issuer: AccountId) -> bool {
        self.assert_testing();
        self._add_sbt_issuer(&issuer)
    }

    #[payable]
    pub fn testing_sbt_mint(
        &mut self,
        issuer: AccountId,
        token_spec: Vec<(AccountId, Vec<TokenMetadata>)>,
    ) -> Vec<TokenId> {
        self.assert_admins();
        self.assert_testing();
        self._sbt_mint(&issuer, token_spec)
    }

    pub fn testing_sbt_renew(&mut self, issuer: AccountId, tokens: Vec<TokenId>, expires_at: u64) {
        self.assert_admins();
        self.assert_testing();
        self._sbt_renew(issuer, tokens, expires_at)
    }
}
