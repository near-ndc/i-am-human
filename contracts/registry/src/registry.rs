// TODO: remove allow unused_variables
#![allow(unused_variables)]

use std::collections::HashMap;

use near_sdk::{near_bindgen, AccountId};

use crate::*;
use cost::*;
use sbt::*;

const MAX_LIMIT: u32 = 1000;

#[near_bindgen]
impl SBTRegistry for Contract {
    /**********
     * QUERIES
     **********/

    fn sbt(&self, issuer: AccountId, token: TokenId) -> Option<Token> {
        let issuer_id = self.assert_issuer(&issuer);
        self.issuer_tokens
            .get(&IssuerTokenId { issuer_id, token })
            .map(|td| td.to_token(token))
    }

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

        return self.supply_by_owner.get(&(account, issuer_id)).unwrap_or(0);
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

        let mut resp = Vec::new();
        for token in from_token..max_id {
            if let Some(t) = self.issuer_tokens.get(&IssuerTokenId { issuer_id, token }) {
                resp.push(t.to_token(token))
            }
        }
        return resp;
    }

    /// Query SBT tokens by owner
    /// If `from_class` is not specified, then `from_class` should be assumed to be the first
    /// valid class id.
    /// If limit is not specified, default is used: 100.
    /// Returns list of pairs: `(Contract address, list of token IDs)`.
    fn sbt_tokens_by_owner(
        &self,
        account: AccountId,
        issuer: Option<AccountId>,
        from_class: Option<u64>,
        limit: Option<u32>,
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
        let mut from_class = from_class.unwrap_or(0);
        // iter_from starts from exclusive "left end"
        if from_class != 0 {
            from_class -= 1;
        }
        let mut limit = limit.unwrap_or(MAX_LIMIT);
        require!(limit > 0, "limit must be bigger than 0");

        let mut resp = Vec::new();
        let mut tokens = Vec::new();
        let mut prev_issuer = issuer_id;

        for (key, token_id) in self
            .balances
            .iter_from(balance_key(account.clone(), issuer_id, from_class))
            .take(limit as usize)
        {
            // TODO: maybe we should continue the scan?
            if key.owner != account {
                break;
            }
            if prev_issuer != key.issuer_id {
                if issuer_id != 0 {
                    break;
                }
                if tokens.len() > 0 {
                    let issuer = self.issuer_by_id(prev_issuer);
                    resp.push((issuer, tokens));
                    tokens = Vec::new();
                }
                prev_issuer = key.issuer_id;
            }
            let t = self.get_token(key.issuer_id, token_id);
            tokens.push(OwnedToken {
                token: token_id,
                metadata: t.metadata.v1(),
            });
            limit = limit - 1;
            if limit == 0 {
                break;
            }
        }
        if prev_issuer != 0 && tokens.len() > 0 {
            let issuer = self.issuer_by_id(prev_issuer);
            resp.push((issuer, tokens));
        }
        return resp;
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
    #[payable]
    fn sbt_mint(&mut self, token_spec: Vec<(AccountId, Vec<TokenMetadata>)>) -> Vec<TokenId> {
        let storage_start = env::storage_usage();
        let storage_deposit = env::attached_deposit();
        require!(
            storage_deposit >= 6 * MILI_NEAR,
            "min required storage deposit: 0.006 NEAR"
        );

        let issuer = &env::predecessor_account_id();
        let issuer_id = self.assert_issuer(issuer);
        let mut num_tokens = 0;
        for el in token_spec.iter() {
            num_tokens += el.1.len() as u64;
        }
        let mut token = self.next_token_id(issuer_id, num_tokens);
        let ret_token_ids = (token..token + num_tokens).collect();
        let mut supply_by_class = HashMap::new();
        let mut per_recipient: HashMap<AccountId, Vec<TokenId>> = HashMap::new();

        for (owner, metadatas) in token_spec {
            // no need to check ongoing_soult_tx, because it will automatically ban the source account
            self.assert_not_banned(&owner);

            let recipient_tokens = per_recipient.entry(owner.clone()).or_default();
            let metadatas_len = metadatas.len();

            for metadata in metadatas {
                let prev = self.balances.insert(
                    &balance_key(owner.clone(), issuer_id, metadata.class),
                    &token,
                );
                require!(
                    prev.is_none(),
                    format! {"{} already has SBT of class {}", owner, metadata.class}
                );

                // update supply by class
                match supply_by_class.get_mut(&metadata.class) {
                    None => {
                        supply_by_class.insert(metadata.class, 1);
                    }
                    Some(s) => *s = *s + 1,
                };

                self.issuer_tokens.insert(
                    &IssuerTokenId { issuer_id, token },
                    &TokenData {
                        owner: owner.clone(),
                        metadata: metadata.into(),
                    },
                );
                recipient_tokens.push(token);

                token += 1;
            }

            // update supply by owner
            let skey = (owner, issuer_id);
            let sowner = self.supply_by_owner.get(&skey).unwrap_or(0) + metadatas_len as u64;
            self.supply_by_owner.insert(&skey, &sowner);
        }

        for (cls, new_supply) in supply_by_class {
            let key = (issuer_id, cls);
            let s = self.supply_by_class.get(&key).unwrap_or(0) + new_supply;
            self.supply_by_class.insert(&key, &s);
        }

        let new_supply = self.supply_by_issuer.get(&issuer_id).unwrap_or(0) + num_tokens;
        self.supply_by_issuer.insert(&issuer_id, &new_supply);

        let mut minted: Vec<(&AccountId, &Vec<TokenId>)> = per_recipient.iter().collect();
        minted.sort_by(|a, b| a.0.cmp(b.0));
        SbtMint {
            issuer: &issuer,
            tokens: minted,
        }
        .emit();

        let required_deposit =
            (env::storage_usage() - storage_start) as u128 * env::storage_byte_cost();
        require!(
            storage_deposit >= required_deposit,
            format!(
                "not enough NEAR storage depost, required: {}",
                required_deposit
            )
        );

        ret_token_ids
    }

    /// sbt_recover reassigns all tokens issued by the caller, from the old owner to a new owner.
    /// Adds `old_owner` to a banned accounts list.
    /// Caller must be a valid issuer.
    /// Must be called by a valid SBT issuer.
    /// Must emit `Recover` event.
    /// Must be called by an operator.
    /// Must provide enough NEAR to cover registry storage cost.
    /// Requires attaching enough tokens to cover the storage growth.
    #[payable]
    fn sbt_recover(&mut self, from: AccountId, to: AccountId) {
        let issuer = env::predecessor_account_id();
        let issuer_id = self.assert_issuer(&issuer);
        self.assert_not_banned(&from);
        self.assert_not_banned(&to);

        //todo: check if enough near was provided to cover the storage
        let storage_deposit = env::attached_deposit();
        require!(
            storage_deposit >= 6 * MILI_NEAR,
            "min required storage deposit: 0.006 NEAR"
        );

        //todo: get all tokens issued by the caller, where the owner == from
        let tokens: Vec<OwnedToken> = self.sbt_tokens_by_owner(from, Some(issuer), None, None)[0].1;

        //todo: reassign all tokens issued by the caller, from the old owner to a new owner.
        for token in &tokens {
            let token = token.clone().token;
            let mut t = self.get_token(issuer_id, token);
            self.assert_not_banned(&t.owner);
            t.owner = to;
            self.issuer_tokens
                .insert(&IssuerTokenId { issuer_id, token }, &t);
        }

        //todo: add old_owner to a bannded list
        self.banlist.insert(&from);

        //todo: emit Recover event
        SbtRecover {
            issuer: &issuer,
            old_owner: &from,
            new_owner: &to,
        };

        // no need to check ongoing_soult_tx, because it will automatically ban the source account

        env::panic_str("not implemented");
        // add events
    }

    /// sbt_renew will update the expire time of provided tokens.
    /// `expires_at` is a unix timestamp (in seconds).
    /// Must be called by an SBT contract.
    /// Must emit `Renew` event.
    /// Use `cost::renew_gas` to calculate expected amount of gas that should be assigned for this
    /// function
    fn sbt_renew(&mut self, tokens: Vec<TokenId>, expires_at: u64) {
        let issuer = env::predecessor_account_id();
        let issuer_id = self.assert_issuer(&issuer);
        for token in &tokens {
            let token = token.clone();
            let mut t = self.get_token(issuer_id, token);
            self.assert_not_banned(&t.owner);
            let mut m = t.metadata.v1();
            m.expires_at = Some(expires_at);
            t.metadata = m.into();
            self.issuer_tokens
                .insert(&IssuerTokenId { issuer_id, token }, &t);
        }
        SbtTokensEvent { issuer, tokens }.emit_renew();
    }

    /// Revokes SBT by burning it.
    /// Must be called by an SBT contract.
    /// Must emit one of `Revoke` or `Burn` event.
    /// Returns true if a token is a valid, active SBT. Otherwise returns false.
    fn sbt_revoke(&mut self, token: Vec<TokenId>) -> bool {
        let issuer = env::predecessor_account_id();
        self.assert_issuer(&issuer);
        env::panic_str("not implemented");
        // add events
    }
}
