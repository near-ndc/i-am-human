// TODO: remove allow unused_variables
#![allow(dead_code)]
#![allow(unused_variables)]

use near_sdk::{env::account_balance, near_bindgen, AccountId};

use crate::*;
use sbt::*;

const MAX_LIMIT: u32 = 1000;

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
        metadata: mock_metadata(),
    }
}

fn mock_metadata() -> TokenMetadata {
    TokenMetadata {
        class: 100,
        issued_at: Some(1680513165),
        expires_at: Some(1685776365),
        reference: None,      //Some("https://somelink.com/mydoc".to_owned()),
        reference_hash: None, // Some(vec![232, 200, 7, 74, 151, 212, 112, 108, 102, 57, 160, 89, 106, 36, 58, 72, 115, 35, 35, 116, 169, 31, 38, 54, 155, 44, 149, 74, 78, 145, 209, 35,] .into(),),
    }
}

// Implement the contract structure
#[near_bindgen]
impl SBTRegistry for Contract {
    /**********
     * QUERIES
     **********/

    fn sbt(&self, ctr: AccountId, token: TokenId) -> Option<Token> {
        let ctr_id = self.ctr_id(&ctr);
        self.ctr_tokens
            .get(&CtrTokenId { ctr_id, token })
            .map(|td| td.to_token(token))
    }

    // NOTE: we don't delete tokens on revoke, so we can get the supply in an easy way.
    fn sbt_supply(&self, ctr: AccountId) -> u64 {
        let ctr_id = match self.sbt_contracts.get(&ctr) {
            None => return 0,
            Some(id) => id,
        };
        self.next_token_ids.get(&ctr_id).unwrap_or(0)
    }

    /// returns total amount of tokens of given class minted by this contract
    fn sbt_supply_by_class(&self, ctr: AccountId, class: ClassId) -> u64 {
        // TODO: maybe remove, or make it optional?
        2
    }

    /// returns total supply of SBTs for a given owner.
    /// If class is specified, returns only owner supply of the given class -- must be 0 or 1.
    fn sbt_supply_by_owner(
        &self,
        account: AccountId,
        ctr: AccountId,
        class: Option<ClassId>,
    ) -> u64 {
        if self.ongoing_soul_tx.contains_key(&account) {
            return 0;
        }

        let ctr_id = match self.sbt_contracts.get(&ctr) {
            // early return if the class is not registered
            None => return 0,
            Some(id) => id,
        };
        if let Some(class_id) = class {
            return match self.balances.contains_key(&BalanceKey {
                owner: account,
                ctr_id,
                class_id,
            }) {
                true => 1,
                _ => 0,
            };
        }

        return self.supply_by_owner.get(&(account, ctr_id)).unwrap_or(0);
    }

    /// Query sbt tokens issued by a given contract.
    /// If `from_token` is not specified, then `from_token` should be assumed
    /// to be the first valid token id.
    /// If limit is not specified, default is used: 100.
    fn sbt_tokens(
        &self,
        ctr: AccountId,
        from_token: Option<u64>,
        limit: Option<u32>,
    ) -> Vec<Token> {
        // TODO
        vec![mock_token_str(1, "alice.near")]
    }

    /// Query SBT tokens by owner
    /// If `from_class` is not specified, then `from_class` should be assumed to be the first
    /// valid class id.
    /// If limit is not specified, default is used: 100.
    /// Returns list of pairs: `(Contract address, list of token IDs)`.
    fn sbt_tokens_by_owner(
        &self,
        account: AccountId,
        ctr: Option<AccountId>,
        from_class: Option<u64>,
        limit: Option<u32>,
    ) -> Vec<(AccountId, Vec<OwnedToken>)> {
        if from_class.is_some() {
            require!(
                ctr.is_some(),
                "ctr must be defined if from_class is defined"
            );
        }
        if self.ongoing_soul_tx.contains_key(&account) {
            return vec![];
        }

        let ctr_id = match ctr {
            None => 0,
            Some(addr) => self.ctr_id(&addr),
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
        let mut prev_ctr = ctr_id;

        for (key, token_id) in self
            .balances
            .iter_from(balance_key(account.clone(), ctr_id, from_class))
            .take(limit as usize)
        {
            if key.owner != account {
                break;
            }
            if prev_ctr != key.ctr_id {
                if ctr_id != 0 {
                    break;
                }
                if tokens.len() > 0 {
                    let issuer = self
                        .ctr_id_map
                        .get(&prev_ctr)
                        .expect("internal error: inconsistent sbt issuer map");
                    resp.push((issuer, tokens));
                    tokens = Vec::new();
                }
                prev_ctr = key.ctr_id;
            }
            let t = self
                .ctr_tokens
                .get(&CtrTokenId {
                    ctr_id: key.ctr_id,
                    token: token_id,
                })
                .expect("internal error: token data not found");
            tokens.push(OwnedToken {
                token: token_id,
                metadata: t.metadata.v1(),
            });
            limit = limit - 1;
            if limit == 0 {
                break;
            }
        }
        if prev_ctr != 0 && tokens.len() > 0 {
            let issuer = self.ctr_id_map.get(&prev_ctr).unwrap();
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
        // TODO: deposit check

        let ctr = &env::predecessor_account_id();
        let ctr_id = self.ctr_id(ctr);
        let mut num_tokens = 0;
        for el in token_spec.iter() {
            num_tokens += el.1.len() as u64;
        }
        let mut token = self.next_token_id(ctr_id, num_tokens);
        let ret_token_ids = (token..token + num_tokens).collect();

        for (owner, metadatas) in token_spec {
            for metadata in metadatas {
                self.assert_not_banned(&owner);

                let prev = self.balances.insert(
                    &BalanceKey {
                        owner: owner.clone(),
                        ctr_id,
                        class_id: metadata.class,
                    },
                    &token,
                );
                require!(
                    prev.is_none(),
                    format! {"{} already has SBT of class {}", &owner, metadata.class}
                );

                self.ctr_tokens.insert(
                    &CtrTokenId { ctr_id, token },
                    &TokenData {
                        owner: owner.clone(),
                        metadata: metadata.into(),
                    },
                );
                // TODO use struct instead of pair
                let skey = (owner.clone(), ctr_id);
                let sowner = self.supply_by_owner.get(&skey).unwrap_or(0) + 1;
                self.supply_by_owner.insert(&skey, &sowner);

                token += 1;
            }
        }

        // TODO: emit events

        ret_token_ids
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
        env::panic_str("not implemented");
    }

    /// sbt_renew will update the expire time of provided tokens.
    /// `expires_at` is a unix timestamp (in seconds).
    /// Must be called by an SBT contract.
    /// Must emit `Renew` event.
    fn sbt_renew(&mut self, tokens: Vec<TokenId>, expires_at: u64) {
        let ctr = env::predecessor_account_id();
        self.assert_issuer(&ctr);
        env::panic_str("not implemented");
    }

    /// Revokes SBT, could potentially burn it or update the expire time.
    /// Must be called by an SBT contract.
    /// Must emit one of `Revoke` or `Burn` event.
    /// Returns true if a token is a valid, active SBT. Otherwise returns false.
    fn sbt_revoke(&mut self, token: u64) -> bool {
        let ctr = env::predecessor_account_id();
        self.assert_issuer(&ctr);
        env::panic_str("not implemented");
    }
}
