use std::collections::HashMap;

use near_sdk::{near_bindgen, AccountId};

use crate::*;

const MAX_LIMIT: u32 = 1000;

#[near_bindgen]
impl SBTRegistry for Contract {
    /**********
     * QUERIES
     **********/

    /// returns the token, if it does not exist returns None
    fn sbt(&self, issuer: AccountId, token: TokenId) -> Option<Token> {
        let issuer_id = self.assert_issuer(&issuer);
        self.issuer_tokens
            .get(&IssuerTokenId { issuer_id, token })
            .map(|td| td.to_token(token))
    }

    /// returns total amount of tokens minted by the given issuer
    fn sbt_supply(&self, issuer: AccountId) -> u64 {
        let issuer_id = match self.sbt_issuers.get(&issuer) {
            None => return 0,
            Some(id) => id,
        };
        self.supply_by_issuer.get(&issuer_id).unwrap_or(0)
    }

    /// returns total amount of tokens of given class minted by this contract
    fn sbt_supply_by_class(&self, issuer: AccountId, class: ClassId) -> u64 {
        let issuer_id = match self.sbt_issuers.get(&issuer) {
            None => return 0,
            Some(id) => id,
        };
        self.supply_by_class.get(&(issuer_id, class)).unwrap_or(0)
    }

    /// returns total supply of SBTs for a given owner.
    /// If class is specified, returns only owner supply of the given class -- must be 0 or 1.
    fn sbt_supply_by_owner(
        &self,
        account: AccountId,
        issuer: AccountId,
        class: Option<ClassId>,
    ) -> u64 {
        // we don't check banlist because we should still enable banned accounts to query their tokens
        if self.ongoing_soul_tx.contains_key(&account) {
            return 0;
        }

        let issuer_id = match self.sbt_issuers.get(&issuer) {
            // early return if the class is not registered
            None => return 0,
            Some(id) => id,
        };
        if let Some(class_id) = class {
            return match self
                .balances
                .contains_key(&balance_key(account, issuer_id, class_id))
            {
                true => 1,
                _ => 0,
            };
        }

        self.supply_by_owner.get(&(account, issuer_id)).unwrap_or(0)
    }

    /// Query sbt tokens issued by a given contract.
    /// If `from_token` is not specified, then `from_token` should be assumed
    /// to be the first valid token id.
    /// The function search tokens sequentially. So, if empty list is returned, then a user
    /// should continue querying the contract by setting `from_token = previous from_token + limit`
    /// until the `from_token > sbt_supply(issuer)`.
    /// If limit is not specified, default is used: 1000.
    fn sbt_tokens(
        &self,
        issuer: AccountId,
        from_token: Option<u64>,
        limit: Option<u32>,
        with_expired: Option<bool>,
    ) -> Vec<Token> {
        let issuer_id = match self.sbt_issuers.get(&issuer) {
            None => return vec![],
            Some(i) => i,
        };
        let from_token = from_token.unwrap_or(1);
        require!(from_token > 0, "from_token, if set, must be >= 1");
        let limit = limit.unwrap_or(MAX_LIMIT);
        require!(limit > 0, "limit must be bigger than 0");
        let mut max_id = self.next_token_ids.get(&issuer_id).unwrap_or(0);
        if max_id < from_token {
            return vec![];
        }
        max_id = std::cmp::min(max_id + 1, from_token + limit as u64);

        let now = env::block_timestamp_ms();
        let non_expired = !with_expired.unwrap_or(false);
        let mut resp = Vec::new();
        for token in from_token..max_id {
            if let Some(t) = self.issuer_tokens.get(&IssuerTokenId { issuer_id, token }) {
                if non_expired && t.metadata.expires_at().unwrap_or(now) < now {
                    continue;
                }
                resp.push(t.to_token(token))
            }
        }
        resp
    }

    /// Query SBT tokens by owner
    /// If `from_class` is not specified, then `from_class` should be assumed to be the first
    /// valid class id.
    /// If limit is not specified, default is used: MAX_LIMIT.
    /// Returns list of pairs: `(Issuer address, list of token IDs)`.
    /// if `with_expired` is set to `true` then only non-expired tokens are returned, otherwise all tokens are returned.
    fn sbt_tokens_by_owner(
        &self,
        account: AccountId,
        issuer: Option<AccountId>,
        from_class: Option<u64>,
        limit: Option<u32>,
        with_expired: Option<bool>,
    ) -> Vec<(AccountId, Vec<OwnedToken>)> {
        if from_class.is_some() {
            require!(
                issuer.is_some(),
                "issuer must be defined if from_class is defined"
            );
        }
        // we don't check banlist because we should still enable banned accounts to query their tokens
        if self.ongoing_soul_tx.contains_key(&account) {
            return vec![];
        }

        let issuer_id = match issuer {
            None => 0,
            // use self.sbt_contracts.get when changing to query by issuer_start
            Some(addr) => self.assert_issuer(&addr),
        };
        let from_class = from_class.unwrap_or(0);
        let mut limit = limit.unwrap_or(MAX_LIMIT);
        require!(limit > 0, "limit must be bigger than 0");

        let mut resp = Vec::new();
        let mut tokens = Vec::new();
        let mut prev_issuer = issuer_id;

        let now = env::block_timestamp_ms();
        let with_expired = with_expired.unwrap_or(false);

        for (key, token_id) in self.balances.iter() {
            if key.class_id < from_class || key.issuer_id < issuer_id || key.owner != account {
                continue;
            }
            if prev_issuer != key.issuer_id {
                if issuer_id != 0 {
                    break;
                }
                if !tokens.is_empty() {
                    let issuer = self.issuer_by_id(prev_issuer);
                    resp.push((issuer, tokens));
                    tokens = Vec::new();
                }
                prev_issuer = key.issuer_id;
            }
            let t: TokenData = self.get_token(key.issuer_id, token_id);
            if !with_expired && t.metadata.expires_at().unwrap_or(now) < now {
                continue;
            }
            tokens.push(OwnedToken {
                token: token_id,
                metadata: t.metadata.v1(),
            });
            limit -= 1;
            if limit == 0 {
                break;
            }
        }
        if prev_issuer != 0 && !tokens.is_empty() {
            let issuer = self.issuer_by_id(prev_issuer);
            resp.push((issuer, tokens));
        }
        resp
    }

    /// checks if an `account` was banned by the registry.
    fn is_banned(&self, account: AccountId) -> bool {
        self._is_banned(&account)
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
    /// Panics with "out of gas" if token_spec vector is too long and not enough gas was
    /// provided.
    #[payable]
    fn sbt_mint(&mut self, token_spec: Vec<(AccountId, Vec<TokenMetadata>)>) -> Vec<TokenId> {
        let issuer = &env::predecessor_account_id();
        self._sbt_mint(issuer, token_spec)
    }

    /// sbt_recover reassigns all tokens issued by the caller, from the old owner to a new owner.
    /// + Must be called by a valid SBT issuer.
    /// + Must emit `Recover` event once all the tokens have been recovered.
    /// + Requires attaching enough tokens to cover the storage growth.
    /// + Returns the amount of tokens recovered and a boolean: `true` if the whole
    ///   process has finished, `false` when the process has not finished and should be
    ///   continued by a subsequent call.
    /// + User must keep calling the `sbt_recover` until `true` is returned.
    #[payable]
    fn sbt_recover(&mut self, from: AccountId, to: AccountId) -> (u32, bool) {
        self._sbt_recover(from, to, 20)
    }

    /// sbt_renew will update the expire time of provided tokens.
    /// `expires_at` is a unix timestamp miliseconds.
    /// Must be called by an SBT contract.
    /// Must emit `Renew` event.
    /// Use `cost::renew_gas` to calculate expected amount of gas that should be assigned for this
    /// function
    fn sbt_renew(&mut self, tokens: Vec<TokenId>, expires_at: u64) {
        let issuer = env::predecessor_account_id();
        self._sbt_renew(issuer, tokens, expires_at);
    }

    /// Revokes SBT.
    /// Must be called by an SBT contract.
    /// Must emit `Revoke` event.
    /// Must also emit `Burn` event if the SBT tokens are burned (removed).
    fn sbt_revoke(&mut self, tokens: Vec<TokenId>, burn: bool) {
        let issuer = env::predecessor_account_id();
        let issuer_id = self.assert_issuer(&issuer);
        if burn == true {
            let mut revoked_per_class: HashMap<u64, u64> = HashMap::new();
            let mut revoked_per_owner: HashMap<AccountId, u64> = HashMap::new();
            let tokens_burned: u64 = tokens.len().try_into().unwrap();
            for token in tokens.clone() {
                // update balances
                let token_object = self.get_token(issuer_id, token);
                let owner = token_object.owner;
                let class_id = token_object.metadata.class_id();
                let balance_key = &BalanceKey {
                    issuer_id,
                    owner: owner.clone(),
                    class_id,
                };
                self.balances.remove(balance_key);

                // collect the info about the tokens revoked per owner and per class
                // to update the balances accordingly
                revoked_per_class
                    .entry(class_id)
                    .and_modify(|key_value| *key_value += 1)
                    .or_insert(1);
                revoked_per_owner
                    .entry(owner)
                    .and_modify(|key_value| *key_value += 1)
                    .or_insert(1);

                // remove from issuer_tokens
                self.issuer_tokens
                    .remove(&IssuerTokenId { issuer_id, token });
            }

            // update supply by owner
            for (owner_id, tokens_revoked) in revoked_per_owner {
                let old_supply = self
                    .supply_by_owner
                    .get(&(owner_id.clone(), issuer_id))
                    .unwrap();
                self.supply_by_owner
                    .insert(&(owner_id, issuer_id), &(old_supply - &tokens_revoked));
            }

            // update supply by class
            for (class_id, tokens_revoked) in revoked_per_class {
                let old_supply = self.supply_by_class.get(&(issuer_id, class_id)).unwrap();
                self.supply_by_class
                    .insert(&(issuer_id, class_id), &(&old_supply - &tokens_revoked));
            }

            // update supply by issuer
            let supply_by_issuer = self.supply_by_issuer.get(&(issuer_id)).unwrap_or(0);
            self.supply_by_issuer
                .insert(&(issuer_id), &(supply_by_issuer - tokens_burned));

            // emit event
            SbtTokensEvent {
                issuer: issuer.clone(),
                tokens: tokens.clone(),
            }
            .emit_burn();
        } else {
            let current_timestamp = env::block_timestamp();
            // revoke
            for token in tokens.clone() {
                // update expire date for all tokens to current_timestamp
                let mut t = self.get_token(issuer_id, token);
                let mut m = t.metadata.v1();
                m.expires_at = Some(current_timestamp);
                t.metadata = m.into();
                self.issuer_tokens
                    .insert(&IssuerTokenId { issuer_id, token }, &t);
            }
        }
        SbtTokensEvent { issuer, tokens }.emit_revoke();
    }
}
