use std::collections::HashSet;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, TreeMap, UnorderedMap, UnorderedSet};
use near_sdk::{env, near_bindgen, require, AccountId, PanicOnDefault};

use sbt::{emit_soul_transfer, ClassId, SBTRegistry, SbtTokensEvent, TokenData, TokenId};

use crate::storage::*;

mod registry;
mod storage;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// Registry admin, expected to be a DAO.
    pub authority: AccountId,

    /// registry of approved SBT contracts to issue tokens
    pub sbt_issuers: UnorderedMap<AccountId, IssuerId>,
    pub issuer_id_map: LookupMap<IssuerId, AccountId>, // reverse index
    /// registry of blacklisted accounts by issuer
    pub(crate) banlist: UnorderedSet<AccountId>,
    /// store ongoing soul transfers by "old owner"
    pub(crate) ongoing_soul_tx: LookupMap<AccountId, IssuerTokenId>,

    pub(crate) supply_by_owner: LookupMap<(AccountId, IssuerId), u64>,
    pub(crate) supply_by_class: LookupMap<(IssuerId, ClassId), u64>,
    pub(crate) supply_by_issuer: LookupMap<IssuerId, u64>,

    pub(crate) balances: TreeMap<BalanceKey, TokenId>,
    pub(crate) issuer_tokens: LookupMap<IssuerTokenId, TokenData>,

    /// map of SBT contract -> next available token_id
    pub(crate) next_token_ids: LookupMap<IssuerId, TokenId>,
    pub(crate) next_issuer_id: IssuerId,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(authority: AccountId) -> Self {
        Self {
            authority,
            sbt_issuers: UnorderedMap::new(StorageKey::SbtIssuers),
            issuer_id_map: LookupMap::new(StorageKey::SbtIssuersRev),
            banlist: UnorderedSet::new(StorageKey::Banlist),
            supply_by_owner: LookupMap::new(StorageKey::SupplyByOwner),
            supply_by_class: LookupMap::new(StorageKey::SupplyByClass),
            supply_by_issuer: LookupMap::new(StorageKey::SupplyByIssuer),
            balances: TreeMap::new(StorageKey::Balances),
            issuer_tokens: LookupMap::new(StorageKey::IssuerTokens),
            next_token_ids: LookupMap::new(StorageKey::NextTokenId),
            next_issuer_id: 1,
            ongoing_soul_tx: LookupMap::new(StorageKey::OngoingSoultTx),
        }
    }

    //
    // Queries
    //

    pub fn sbt_contracts(&self) -> Vec<AccountId> {
        self.sbt_issuers.keys().collect()
    }

    #[inline]
    fn _is_banned(&self, account: &AccountId) -> bool {
        self.banlist.contains(account)
    }

    //
    // Transactions
    //

    /// Transfers atomically all SBT tokens from one account to another account.
    /// The caller must be an SBT holder and the `to` must not be a banned account.
    /// Returns the amount of tokens transferred and a boolean: `true` if the whole
    /// process has finished, `false` when the process has not
    /// finished and should be continued by a subsequent call.
    /// User must keeps calling `sbt_soul_transfer` until `true` is returned.
    /// Must emit `SoulTransfer` event.
    #[payable]
    pub fn sbt_soul_transfer(
        &mut self,
        to: AccountId,
        #[allow(unused_variables)] memo: Option<String>,
    ) -> (u32, bool) {
        // TODO: test what is the max safe amount of updates
        self._sbt_soul_transfer(to, 200)
    }

    // execution of the sbt_soult_transfer in this function to parametrize `max_updates` in
    // order to facilitate tests.
    pub(crate) fn _sbt_soul_transfer(&mut self, to: AccountId, limit: usize) -> (u32, bool) {
        let owner = env::predecessor_account_id();
        let (resumed, start) = match self.ongoing_soul_tx.get(&owner) {
            // starting the process
            None => {
                require!(!self._is_banned(&to), "`to` is banned");
                // insert into banlist and assuer owner is not already banned.
                require!(
                    self.banlist.insert(&owner),
                    "caller banned: can't make soul transfer"
                );
                (
                    false,
                    IssuerTokenId {
                        issuer_id: 0,
                        token: 0, // NOTE: this is class ID
                    },
                )
            }
            // resuming Soul Transfer process
            Some(s) => (true, s),
        };

        let batch: Vec<(BalanceKey, TokenId)> = self
            .balances
            .iter_from(BalanceKey {
                owner: owner.clone(),
                issuer_id: start.issuer_id,
                class_id: start.token,
            })
            .take(limit)
            .collect();

        let mut b_new = BalanceKey {
            owner: to.clone(),
            issuer_id: 0,
            class_id: 0,
        };
        let mut prev_issuer: IssuerId = 0;
        let mut i = 0;
        for (b, tid) in &batch {
            if b.owner != owner {
                break;
            }
            i += 1;

            if prev_issuer != b.issuer_id {
                prev_issuer = b.issuer_id;
                // update user token supply map
                match self.supply_by_owner.remove(&(owner.clone(), prev_issuer)) {
                    None => (),
                    Some(supply_owner) => {
                        let key = &(to.clone(), prev_issuer);
                        let supply_to = self.supply_by_owner.get(key).unwrap_or(0);
                        self.supply_by_owner
                            .insert(key, &(supply_owner + supply_to));
                    }
                }
            }

            self.balances.remove(b);
            b_new.issuer_id = b.issuer_id;
            b_new.class_id = b.class_id;
            // TODO: decide if we should overwrite or panic if receipient already had a token.
            // now we overwrite.
            self.balances.insert(&b_new, tid);
            self.balances.remove(&b);

            let i_key = IssuerTokenId {
                issuer_id: b.issuer_id,
                token: tid.clone(),
            };
            let mut td = self.issuer_tokens.get(&i_key).unwrap();
            td.owner = to.clone();
            self.issuer_tokens.insert(&i_key, &td);
        }

        let completed = i != limit;
        if completed {
            if resumed {
                // insert is happening when we need to continue, so don't need to remove if
                // the process finishes in the same transaction.
                self.ongoing_soul_tx.remove(&owner);
            }
            // we emit the event only once the operation is completed and only if some tokens were
            // transferred
            if resumed || i > 0 {
                emit_soul_transfer(&owner, &to);
            }
        } else {
            let last = &batch[i];
            self.ongoing_soul_tx.insert(
                &owner,
                &IssuerTokenId {
                    issuer_id: last.0.issuer_id,
                    token: last.1,
                },
            );
        }
        // edge case: caller doesn't have any token or resumed by but there is no more tokens
        // to transfer
        if i == 0 {
            return (0, false);
        }

        return (i as u32, completed);
    }

    pub fn sbt_burn(
        &mut self,
        issuer: AccountId,
        tokens: Vec<TokenId>,
        #[allow(unused_variables)] memo: Option<String>,
    ) {
        let owner = env::predecessor_account_id();
        require!(
            !self.ongoing_soul_tx.contains_key(&owner),
            "can't burn tokens while in soul_transfer"
        );

        let issuer_id = self.assert_issuer(&issuer);
        let token_len = tokens.len() as u64;
        let mut token_ids = HashSet::new();
        for tid in tokens.iter() {
            require!(
                !token_ids.contains(tid),
                format!("duplicated token_id in tokens: {}", tid)
            );
            token_ids.insert(tid);

            let ct_key = &IssuerTokenId {
                issuer_id,
                token: *tid,
            };
            let t = self
                .issuer_tokens
                .get(ct_key)
                .expect(&format!("tokenID={} not found", tid));
            require!(
                t.owner == owner,
                &format!("not an owner of tokenID={}", tid)
            );

            self.issuer_tokens.remove(ct_key);
            let class_id = t.metadata.v1().class;
            self.balances
                .remove(&balance_key(owner.clone(), issuer_id, class_id));

            // update supply by class
            let key = (issuer_id, class_id);
            let mut supply = self.supply_by_class.get(&key).unwrap();
            supply -= 1;
            self.supply_by_class.insert(&key, &supply);
        }

        // update supply by owner
        let key = (owner, issuer_id);
        let mut supply = self.supply_by_owner.get(&key).unwrap();
        supply -= token_len;
        self.supply_by_owner.insert(&key, &supply);

        // update total supply by issuer
        let mut supply = self.supply_by_issuer.get(&issuer_id).unwrap();
        supply -= token_len;
        self.supply_by_issuer.insert(&issuer_id, &supply);

        SbtTokensEvent { issuer, tokens }.emit_burn();
    }

    //
    // Authority
    //

    /// returns false if the `issuer` contract was already registered.
    pub fn admin_add_sbt_issuer(&mut self, issuer: AccountId) -> bool {
        self.assert_authority();
        let previous = self.sbt_issuers.insert(&issuer, &self.next_issuer_id);
        self.issuer_id_map.insert(&self.next_issuer_id, &issuer);
        self.next_issuer_id += 1;
        previous.is_none()
    }

    //
    // Internal
    //

    /// Queries a given token. Panics if token doesn't exist
    pub(crate) fn get_token(&self, issuer_id: IssuerId, token: TokenId) -> TokenData {
        self.issuer_tokens
            .get(&IssuerTokenId { issuer_id, token })
            .expect(&format!("token {} not found", token))
    }

    /// updates the internal token counter based on how many tokens we want to mint (num), and
    /// returns the first valid TokenId for newly minted tokens.
    pub(crate) fn next_token_id(&mut self, issuer_id: IssuerId, num: u64) -> TokenId {
        let tid = self.next_token_ids.get(&issuer_id).unwrap_or(0);
        self.next_token_ids.insert(&issuer_id, &(tid + num));
        tid + 1
    }

    #[inline]
    pub(crate) fn assert_not_banned(&self, owner: &AccountId) {
        require!(
            !self.banlist.contains(owner),
            format!("account {} is banned", owner)
        );
    }

    /// note: use issuer_id() if you need issuer_id
    pub(crate) fn assert_issuer(&self, issuer: &AccountId) -> IssuerId {
        // TODO: use Result rather than panic
        self.sbt_issuers
            .get(issuer)
            .expect("must be called by a registered SBT Issuer")
    }

    pub(crate) fn issuer_by_id(&self, id: IssuerId) -> AccountId {
        self.issuer_id_map
            .get(&id)
            .expect("internal error: inconsistent sbt issuer map")
    }

    pub(crate) fn assert_authority(&self) {
        require!(
            self.authority == env::predecessor_account_id(),
            "not an admin"
        )
    }
}

#[cfg(test)]
mod tests {
    use near_sdk::test_utils::{self, VMContextBuilder};
    use near_sdk::{testing_env, Balance, VMContext};
    use sbt::*;

    use pretty_assertions::assert_eq;

    use super::*;

    fn alice() -> AccountId {
        AccountId::new_unchecked("alice.near".to_string())
    }

    fn alice2() -> AccountId {
        AccountId::new_unchecked("alice.nea".to_string())
    }

    fn bob() -> AccountId {
        AccountId::new_unchecked("bob.near".to_string())
    }

    fn issuer1() -> AccountId {
        AccountId::new_unchecked("sbt.n".to_string())
    }

    fn issuer2() -> AccountId {
        AccountId::new_unchecked("sbt.ne".to_string())
    }

    fn issuer3() -> AccountId {
        AccountId::new_unchecked("sbt.nea".to_string())
    }

    fn issuer4() -> AccountId {
        AccountId::new_unchecked("sbt4.near".to_string())
    }

    fn admin() -> AccountId {
        AccountId::new_unchecked("sbt.near".to_string())
    }

    fn mk_metadata(class: ClassId, expires_at: Option<u64>) -> TokenMetadata {
        TokenMetadata {
            class,
            issued_at: None,
            expires_at,
            reference: Some("abc".to_owned()),
            reference_hash: Some(vec![61, 61].into()),
        }
    }

    fn mk_token(token: TokenId, owner: AccountId, metadata: TokenMetadata) -> Token {
        Token {
            token,
            owner,
            metadata,
        }
    }

    fn mk_owned_token(token: TokenId, metadata: TokenMetadata) -> OwnedToken {
        OwnedToken { token, metadata }
    }

    fn mk_balance_key(owner: AccountId, issuer_id: IssuerId, class_id: ClassId) -> BalanceKey {
        BalanceKey {
            owner,
            issuer_id,
            class_id,
        }
    }

    const START: u64 = 10;
    const MINT_DEPOSIT: Balance = 6 * MILI_NEAR;

    fn setup(predecessor: &AccountId, deposit: Balance) -> (VMContext, Contract) {
        let mut ctx = VMContextBuilder::new()
            .predecessor_account_id(admin())
            // .attached_deposit(deposit_dec.into())
            .block_timestamp(START)
            .is_view(false)
            .build();
        if deposit > 0 {
            ctx.attached_deposit = deposit
        }
        testing_env!(ctx.clone());
        let mut ctr = Contract::new(admin());
        ctr.admin_add_sbt_issuer(issuer1());
        ctr.admin_add_sbt_issuer(issuer2());
        ctr.admin_add_sbt_issuer(issuer3());
        ctx.predecessor_account_id = predecessor.clone();
        testing_env!(ctx.clone());
        return (ctx, ctr);
    }

    #[test]
    fn mint_simple() {
        let (_, mut ctr) = setup(&issuer1(), 2 * MINT_DEPOSIT);
        let m1_1 = mk_metadata(1, Some(START + 10));

        let minted_ids = ctr.sbt_mint(vec![
            (alice(), vec![m1_1.clone()]),
            (bob(), vec![m1_1.clone()]),
        ]);
        assert_eq!(minted_ids, vec![1, 2]);
        assert_eq!(2, ctr.sbt_supply(issuer1()));
        assert_eq!(0, ctr.sbt_supply(issuer2()));

        let sbt1_1 = ctr.sbt(issuer1(), 1).unwrap();
        assert_eq!(sbt1_1, mk_token(1, alice(), m1_1.clone()));
        let sbt1_2 = ctr.sbt(issuer1(), 2).unwrap();
        assert_eq!(sbt1_2, mk_token(2, bob(), m1_1.clone()));
        assert!(ctr.sbt(issuer2(), 1).is_none());
        assert!(ctr.sbt(issuer1(), 3).is_none());

        assert_eq!(1, ctr.sbt_supply_by_owner(alice(), issuer1(), None));
        assert_eq!(1, ctr.sbt_supply_by_owner(alice(), issuer1(), Some(1)));
        assert_eq!(0, ctr.sbt_supply_by_owner(alice(), issuer1(), Some(2)));

        assert_eq!(1, ctr.sbt_supply_by_owner(bob(), issuer1(), None));
        assert_eq!(1, ctr.sbt_supply_by_owner(bob(), issuer1(), Some(1)));
        assert_eq!(0, ctr.sbt_supply_by_owner(bob(), issuer1(), Some(2)));

        let alice_sbts = ctr.sbt_tokens_by_owner(alice(), None, None, None);
        let expected = vec![(issuer1(), vec![mk_owned_token(1, m1_1.clone())])];
        assert_eq!(alice_sbts, expected);

        let bob_sbts = ctr.sbt_tokens_by_owner(bob(), None, None, None);
        let expected = vec![(issuer1(), vec![mk_owned_token(2, m1_1.clone())])];
        assert_eq!(bob_sbts, expected);
    }

    #[test]
    fn mint() {
        let (mut ctx, mut ctr) = setup(&issuer1(), MINT_DEPOSIT);
        let m1_1 = mk_metadata(1, Some(START + 10));
        let m1_2 = mk_metadata(1, Some(START + 12));
        let m2_1 = mk_metadata(2, Some(START + 14));
        let m4_1 = mk_metadata(4, Some(START + 16));

        // mint an SBT to a user with same prefix as alice
        let minted_ids = ctr.sbt_mint(vec![(alice2(), vec![m1_1.clone()])]);
        assert_eq!(minted_ids, vec![1]);
        assert_eq!(
            test_utils::get_logs(),
            mk_log_str(
                "mint",
                &format!(
                    r#"{{"issuer":"{}","tokens":[["{}",[1]]]}}"#,
                    issuer1(),
                    alice2()
                )
            )
        );

        ctx.predecessor_account_id = issuer2();
        ctx.attached_deposit = 4 * MINT_DEPOSIT;
        testing_env!(ctx.clone());
        let minted_ids = ctr.sbt_mint(vec![
            (alice(), vec![m1_1.clone()]),
            (bob(), vec![m1_2.clone()]),
            (alice2(), vec![m1_1.clone()]),
            (alice(), vec![m2_1.clone()]),
        ]);
        assert_eq!(minted_ids, vec![1, 2, 3, 4]);
        assert_eq!(test_utils::get_logs().len(), 1);
        assert_eq!(
            test_utils::get_logs(),
            mk_log_str(
                "mint",
                &format!(
                    r#"{{"issuer":"{}","tokens":[["{}",[3]],["{}",[1,4]],["{}",[2]]]}}"#,
                    issuer2(),
                    alice2(),
                    alice(),
                    bob()
                )
            )
        );

        // mint again for Alice
        let minted_ids = ctr.sbt_mint(vec![(alice(), vec![m4_1.clone()])]);
        assert_eq!(minted_ids, vec![5]);

        // change the issuer and mint new tokens for alice
        ctx.predecessor_account_id = issuer3();
        ctx.attached_deposit = 2 * MINT_DEPOSIT;
        testing_env!(ctx.clone());
        let minted_ids = ctr.sbt_mint(vec![(alice(), vec![m1_1.clone(), m2_1.clone()])]);
        // since we minted with different issuer, the new SBT should start with 1
        assert_eq!(minted_ids, vec![1, 2]);

        assert_eq!(ctr.sbt_supply_by_class(issuer1(), 0), 0);
        assert_eq!(ctr.sbt_supply_by_class(issuer1(), 1), 1);
        assert_eq!(ctr.sbt_supply_by_class(issuer1(), 2), 0);
        assert_eq!(ctr.sbt_supply_by_class(issuer2(), 1), 3);
        assert_eq!(ctr.sbt_supply_by_class(issuer2(), 2), 1);
        assert_eq!(ctr.sbt_supply_by_class(issuer2(), 3), 0);
        assert_eq!(ctr.sbt_supply_by_class(issuer2(), 4), 1);
        assert_eq!(ctr.sbt_supply_by_class(issuer2(), 5), 0);
        assert_eq!(ctr.sbt_supply_by_class(issuer3(), 1), 1);
        assert_eq!(ctr.sbt_supply_by_class(issuer3(), 2), 1);

        let mut supply_by_issuer = vec![1, 5, 2, 0];
        assert_eq!(ctr.sbt_supply(issuer1()), supply_by_issuer[0]);
        assert_eq!(ctr.sbt_supply(issuer2()), supply_by_issuer[1]);
        assert_eq!(ctr.sbt_supply(issuer3()), supply_by_issuer[2]);
        assert_eq!(ctr.sbt_supply(issuer4()), supply_by_issuer[3]);

        assert_eq!(3, ctr.sbt_supply_by_owner(alice(), issuer2(), None));
        assert_eq!(2, ctr.sbt_supply_by_owner(alice(), issuer3(), None));
        assert_eq!(1, ctr.sbt_supply_by_owner(bob(), issuer2(), None));
        assert_eq!(0, ctr.sbt_supply_by_owner(bob(), issuer3(), None));
        assert_eq!(0, ctr.sbt_supply_by_owner(issuer2(), issuer2(), None));

        let t2_all = vec![
            mk_token(1, alice(), m1_1.clone()),
            mk_token(2, bob(), m1_2.clone()),
            mk_token(3, alice2(), m1_1.clone()),
            mk_token(4, alice(), m2_1.clone()),
            mk_token(5, alice(), m4_1.clone()),
        ];
        let t3_1 = mk_token(1, alice(), m1_1.clone());

        assert_eq!(ctr.sbt(issuer2(), 1).unwrap(), t2_all[0]);
        assert_eq!(ctr.sbt(issuer2(), 2).unwrap(), t2_all[1]);
        assert_eq!(ctr.sbt(issuer2(), 3).unwrap(), t2_all[2]);
        assert_eq!(ctr.sbt(issuer2(), 4).unwrap(), t2_all[3]);
        assert_eq!(ctr.sbt(issuer3(), 1).unwrap(), t3_1);

        // Token checks

        let a_tokens = vec![
            (issuer1(), vec![mk_owned_token(1, m1_1.clone())]),
            (issuer2(), vec![mk_owned_token(3, m1_1.clone())]),
        ];
        assert_eq!(
            &ctr.sbt_tokens_by_owner(alice2(), None, None, None),
            &a_tokens
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice2(), Some(issuer1()), None, None),
            vec![a_tokens[0].clone()],
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice2(), Some(issuer2()), None, None),
            vec![a_tokens[1].clone()]
        );

        let alice_issuer2 = (
            issuer2(),
            vec![
                mk_owned_token(1, m1_1.clone()),
                mk_owned_token(4, m2_1.clone()),
                mk_owned_token(5, m4_1.clone()),
            ],
        );
        let alice_issuer3 = (
            issuer3(),
            vec![
                mk_owned_token(1, m1_1.clone()),
                mk_owned_token(2, m2_1.clone()),
            ],
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), None, None, None),
            vec![alice_issuer2.clone(), alice_issuer3.clone()]
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer2()), None, None),
            vec![alice_issuer2.clone()]
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer3()), None, None),
            vec![alice_issuer3.clone()]
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer2()), Some(1), None),
            vec![alice_issuer2]
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer2()), Some(4), None),
            vec![(issuer2(), vec![mk_owned_token(5, m4_1.clone())])]
        );

        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer1()), Some(5), None),
            vec![]
        );

        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer2()), Some(5), None),
            vec![]
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer3()), Some(1), None),
            vec![alice_issuer3.clone()]
        );

        // check by all tokens
        assert_eq!(
            ctr.sbt_tokens(issuer1(), Some(1), None),
            vec![mk_token(1, alice2(), m1_1.clone())]
        );
        assert_eq!(ctr.sbt_tokens(issuer2(), None, None), t2_all,);
        assert_eq!(ctr.sbt_tokens(issuer2(), None, Some(1)), t2_all[..1]);
        assert_eq!(ctr.sbt_tokens(issuer2(), None, Some(2)), t2_all[..2]);
        assert_eq!(ctr.sbt_tokens(issuer2(), Some(2), Some(2)), t2_all[1..3]);
        assert_eq!(ctr.sbt_tokens(issuer2(), Some(5), Some(5)), t2_all[4..5]);
        assert_eq!(ctr.sbt_tokens(issuer2(), Some(6), Some(2)), vec![]);

        //
        // now let's test buring
        //
        ctx.predecessor_account_id = alice();
        testing_env!(ctx.clone());

        ctr.sbt_burn(issuer2(), vec![1, 5], Some("alice burning".to_owned()));
        assert_eq!(
            test_utils::get_logs(),
            mk_log_str("burn", r#"{"issuer":"sbt.ne","tokens":[1,5]}"#)
        );

        supply_by_issuer[1] -= 2;
        assert_eq!(ctr.sbt_supply(issuer1()), supply_by_issuer[0]);
        assert_eq!(ctr.sbt_supply(issuer2()), supply_by_issuer[1]);
        assert_eq!(ctr.sbt_supply(issuer3()), supply_by_issuer[2]);
        assert_eq!(ctr.sbt_supply(issuer4()), supply_by_issuer[3]);

        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 1);
        assert_eq!(
            ctr.sbt_supply_by_owner(alice(), issuer2(), Some(m2_1.clone().class)),
            1
        );
        assert_eq!(
            ctr.sbt_supply_by_owner(alice(), issuer2(), Some(m1_1.clone().class)),
            0
        );

        let alice_issuer2 = (issuer2(), vec![mk_owned_token(4, m2_1.clone())]);
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), None, None, None),
            vec![alice_issuer2.clone(), alice_issuer3.clone()]
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer2()), None, None),
            vec![alice_issuer2.clone()]
        );
    }

    #[test]
    fn soul_transfer1() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 2 * MINT_DEPOSIT);

        // test1: simple case: alice has one token and she owns alice2 account as well. She
        // will do transfer from alice -> alice2
        let m1_1 = mk_metadata(1, Some(START + 10));
        let m2_1 = mk_metadata(2, Some(START + 10));
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone(), m2_1.clone()])]);

        ctx.predecessor_account_id = issuer2();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone()])]);

        // make soul transfer
        ctx.predecessor_account_id = alice();
        testing_env!(ctx.clone());
        let ret = ctr.sbt_soul_transfer(alice2(), None);
        assert_eq!((3, true), ret);

        assert_eq!(
            test_utils::get_logs(),
            mk_log_str(
                "soul_transfer",
                &format!("{{\"from\":\"{}\",\"to\":\"{}\"}}", alice(), alice2())
            )
        );
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 0);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer1(), None), 2);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer2(), None), 1);

        assert!(ctr.is_banned(alice()));
        assert!(!ctr.is_banned(alice2()));

        assert_eq!(ctr.sbt_tokens_by_owner(alice(), None, None, None), vec![]);
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice2(), None, None, None),
            vec![
                (
                    issuer1(),
                    vec![
                        mk_owned_token(1, m1_1.clone()),
                        mk_owned_token(2, m2_1.clone())
                    ]
                ),
                (issuer2(), vec![mk_owned_token(1, m1_1.clone())]),
            ]
        );

        // tests:
        // + test soult transfer with "continuation" - use the internal _sbt_soul_transfer to control limit
        // + find all edge cases
    }

    #[test]
    fn test_mk_log() {
        let l = mk_log_str("abc", "[1,2,3]");
        assert_eq!(
            l,
            vec![
                r#"EVENT_JSON:{"standard":"nep393","version":"1.0.0","event":"abc","data":[1,2,3]}"#
            ],
        )
    }

    fn mk_log_str(event: &str, data: &str) -> Vec<String> {
        vec![format!(
            "EVENT_JSON:{{\"standard\":\"nep393\",\"version\":\"1.0.0\",\"event\":\"{}\",\"data\":{}}}",
            event,data
        )]
    }

    #[test]
    fn check_tree_iterator() {
        let (_, mut ctr) = setup(&issuer1(), MINT_DEPOSIT);
        ctr.balances.insert(&mk_balance_key(alice2(), 1, 1), &101);
        ctr.balances.insert(&mk_balance_key(alice(), 1, 1), &102);
        ctr.balances.insert(&mk_balance_key(bob(), 1, 1), &103);

        let bs: Vec<(BalanceKey, u64)> = ctr
            .balances
            .iter_from(mk_balance_key(alice(), 0, 0))
            .collect();
        assert_eq!(bs.len(), 2, "bob must be included in the prefix scan");
        assert_eq!(
            bs[0].0.owner,
            alice(),
            "alice must be first in the iterator"
        );
        assert_eq!(bs[0].1, 102, "alice must be first in the iterator");
        assert_eq!(bs[1].0.owner, bob(), "bob must be second in the iterator");
        assert_eq!(bs[1].1, 103, "alice must be first in the iterator");
    }
}
