use std::collections::HashSet;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, TreeMap, UnorderedMap, UnorderedSet};
use near_sdk::{env, near_bindgen, require, AccountId, PanicOnDefault};

use sbt::{
    emit_soul_transfer, ClassId, Nep393Event, SBTRegistry, SbtRecover, SbtTokensEvent, TokenData,
    TokenId,
};

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

    /// maps user balance key to tokenID
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
    /// + The caller must be an SBT holder and the `to` must not be a banned account.
    /// + Returns the amount of tokens transferred and a boolean: `true` if the whole
    ///   process has finished, `false` when the process has not finished and should be
    ///   continued by a subsequent call.
    /// + User must keep calling the `sbt_soul_transfer` until `true` is returned.
    /// + Emits `SoulTransfer` event only once all the tokens that user was in possesion
    ///   of were transfered and at least one token was trasnfered (caller had at least 1 sbt)
    /// + If caller does not have any tokens, nothing will be transfered, the caller
    ///    will be banned and Ban even will be emitted
    #[payable]
    pub fn sbt_soul_transfer(
        &mut self,
        recipient: AccountId,
        #[allow(unused_variables)] memo: Option<String>,
    ) -> (u32, bool) {
        // TODO: test what is the max safe amount of updates
        self._sbt_soul_transfer(recipient, 25)
    }

    // execution of the sbt_soul_transfer in this function to parametrize `max_updates` in
    // order to facilitate tests.
    pub(crate) fn _sbt_soul_transfer(&mut self, recipient: AccountId, limit: usize) -> (u32, bool) {
        let owner = env::predecessor_account_id();

        let (resumed, start) = self.transfer_continuation(&owner, &recipient, true);

        let batch: Vec<(BalanceKey, TokenId)> = self
            .balances
            .iter_from(BalanceKey {
                owner: owner.clone(),
                issuer_id: start.issuer_id,
                class_id: start.token,
            })
            .take(limit)
            .collect();

        let mut key_new = BalanceKey {
            owner: recipient.clone(),
            issuer_id: 0,
            class_id: 0,
        };
        let mut prev_issuer: IssuerId = 0;
        let mut token_counter = 0;
        for (key, token_id) in &batch {
            if key.owner != owner {
                break;
            }
            token_counter += 1;

            if prev_issuer != key.issuer_id {
                prev_issuer = key.issuer_id;
                // update user token supply map
                if let Some(s) = self.supply_by_owner.remove(&(owner.clone(), prev_issuer)) {
                    let key = &(recipient.clone(), prev_issuer);
                    let supply_to = self.supply_by_owner.get(key).unwrap_or(0);
                    self.supply_by_owner.insert(key, &(s + supply_to));
                }
            }

            self.balances.remove(key);
            key_new.issuer_id = key.issuer_id;
            key_new.class_id = key.class_id;
            // TODO: decide if we should overwrite or panic if receipient already had a token.
            // now we overwrite.
            self.balances.insert(&key_new, token_id);
            self.balances.remove(key);

            let i_key = IssuerTokenId {
                issuer_id: key.issuer_id,
                token: *token_id,
            };
            let mut td = self.issuer_tokens.get(&i_key).unwrap();
            td.owner = recipient.clone();
            self.issuer_tokens.insert(&i_key, &td);
        }

        let completed = token_counter != limit;
        if completed {
            if resumed {
                // insert is happening when we need to continue, so don't need to remove if
                // the process finishes in the same transaction.
                self.ongoing_soul_tx.remove(&owner);
            }
            // we emit the event only once the operation is completed and only if some tokens were
            // transferred
            if resumed || token_counter > 0 {
                emit_soul_transfer(&owner, &recipient);
            }
        } else {
            let last = &batch[token_counter - 1];
            self.ongoing_soul_tx.insert(
                &owner,
                &IssuerTokenId {
                    issuer_id: last.0.issuer_id,
                    token: last.0.class_id, // we reuse IssuerTokenId type here (to not generate new code), but we store class_id instead of token here.
                },
            );
        }

        (token_counter as u32, completed)
    }

    pub(crate) fn start_transfer_with_continuation(
        &mut self,
        owner: &AccountId,
        recipient: &AccountId,
        ban_owner: bool,
    ) -> IssuerTokenId {
        require!(
            !self._is_banned(recipient),
            "receiver account is banned. Cannot start the transfer"
        );
        if ban_owner {
            // we only ban the source account in the soul transfer
            // insert into banlist and assure the owner is not already banned.
            require!(
                self.banlist.insert(owner),
                "from account is banned. Cannot start the transfer"
            );
            Nep393Event::Ban(vec![owner]).emit();
        }

        IssuerTokenId {
            issuer_id: 0,
            token: 0, // NOTE: this is class ID
        }
    }

    // If it is the first iteration of the soul transfer, bans the source account, otherwise returns the last transfered token
    fn transfer_continuation(
        &mut self,
        from: &AccountId,
        to: &AccountId,
        ban_owner: bool,
    ) -> (bool, IssuerTokenId) {
        match self.ongoing_soul_tx.get(from) {
            // starting the process
            None => (
                false,
                self.start_transfer_with_continuation(from, to, ban_owner),
            ),
            // resuming sbt_recover process
            Some(s) => (true, s),
        }
    }

    // sbt_recover execution with `limit` parameter in
    // order to facilitate tests.
    fn _sbt_recover(&mut self, from: AccountId, to: AccountId, limit: usize) -> (u32, bool) {
        let storage_start = env::storage_usage();
        let issuer = env::predecessor_account_id();
        let issuer_id = self.assert_issuer(&issuer);
        self.assert_not_banned(&to);
        // get the last transfered token and don't ban the owner.
        let (resumed, start) = self.transfer_continuation(&from, &to, false);

        let mut tokens_recovered = 0;
        let mut class_ids = Vec::new();

        let mut last_token_transfered = BalanceKey {
            owner: from.clone(),
            issuer_id,
            class_id: 0,
        };

        for (key, token) in self
            .balances
            .iter_from(balance_key(from.clone(), start.issuer_id, start.token))
            .take(limit)
        {
            if key.owner != from || key.issuer_id != issuer_id {
                continue;
            }
            tokens_recovered += 1;
            let mut t = self.get_token(key.issuer_id, token);

            class_ids.push(t.metadata.class_id());

            t.owner = to.clone();
            self.issuer_tokens
                .insert(&IssuerTokenId { issuer_id, token }, &t);
            last_token_transfered = key;
        }

        // update user balances
        let mut old_balance_key = balance_key(from.clone(), issuer_id, 0);
        let mut new_balance_key = balance_key(to.clone(), issuer_id, 0);
        for class_id in class_ids {
            old_balance_key.class_id = class_id;
            let token_id = self.balances.remove(&old_balance_key).unwrap();
            new_balance_key.class_id = class_id;
            self.balances.insert(&new_balance_key, &token_id);
        }

        // update supply_by_owner map. We can't do it in the loop above becuse we can't modify
        // self.balances while iterating over it
        let supply_key = &(from.clone(), issuer_id);
        let old_supply_from = self.supply_by_owner.remove(supply_key).unwrap_or(0);
        if old_supply_from != tokens_recovered {
            self.supply_by_owner.insert(
                &(from.clone(), issuer_id),
                &(old_supply_from - tokens_recovered),
            );
        }
        let supply_key = &(to.clone(), issuer_id);
        let old_supply_to = self.supply_by_owner.get(supply_key).unwrap_or(0);
        self.supply_by_owner
            .insert(supply_key, &(old_supply_to + tokens_recovered));

        let completed = tokens_recovered != limit as u64;
        if completed {
            if resumed {
                // insert is happening when we need to continue, so don't need to remove if
                // the process finishes in the same transaction.
                self.ongoing_soul_tx.remove(&from);
            }
            // we emit the event only once the operation is completed and only if some tokens were
            // recovered
            if resumed || tokens_recovered > 0 {
                // emit Recover event
                SbtRecover {
                    issuer: &issuer,
                    old_owner: &from,
                    new_owner: &to,
                }
                .emit();
            }
        } else {
            self.ongoing_soul_tx.insert(
                &from,
                &IssuerTokenId {
                    issuer_id: last_token_transfered.issuer_id,
                    token: last_token_transfered.class_id, // we reuse IssuerTokenId type here (to not generate new code), but we store class_id instead of token here.
                },
            );
        }
        // storage check
        // we are using checked_sub, since the storage can decrease and we are running of risk of underflow
        let storage_usage = env::storage_usage();
        if storage_usage > storage_start {
            let required_deposit =
                (storage_usage - storage_start) as u128 * env::storage_byte_cost();
            require!(
                env::attached_deposit() >= required_deposit,
                format!(
                    "not enough NEAR storage depost, required: {}",
                    required_deposit
                )
            );
        }
        return (tokens_recovered as u32, completed);
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
                .unwrap_or_else(|| panic!("tokenID={} not found", tid));
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

    pub fn change_admin(&mut self, new_admin: AccountId) {
        self.assert_authority();
        self.authority = new_admin;
    }

    //
    // Internal
    //

    /// Queries a given token. Panics if token doesn't exist
    pub(crate) fn get_token(&self, issuer_id: IssuerId, token: TokenId) -> TokenData {
        self.issuer_tokens
            .get(&IssuerTokenId { issuer_id, token })
            .unwrap_or_else(|| panic!("token {} not found", token))
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
    use std::ops::Mul;

    use cost::MILI_NEAR;
    use near_sdk::test_utils::{self, VMContextBuilder};
    use near_sdk::{testing_env, Balance, Gas, VMContext};
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

    fn carol() -> AccountId {
        AccountId::new_unchecked("carol.near".to_string())
    }

    fn dan() -> AccountId {
        AccountId::new_unchecked("dan.near".to_string())
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

    fn mk_batch_metadata(n: u64) -> Vec<TokenMetadata> {
        let mut batch_metadata: Vec<TokenMetadata> = Vec::new();
        for i in 0..n {
            batch_metadata.push(mk_metadata(i, Some(START + i)))
        }
        batch_metadata
    }

    fn max_gas() -> Gas {
        return Gas::ONE_TERA.mul(300);
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

        let alice_sbts = ctr.sbt_tokens_by_owner(alice(), None, None, None, None);
        let expected = vec![(issuer1(), vec![mk_owned_token(1, m1_1.clone())])];
        assert_eq!(alice_sbts, expected);

        let bob_sbts = ctr.sbt_tokens_by_owner(bob(), None, None, None, None);
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
            &ctr.sbt_tokens_by_owner(alice2(), None, None, None, None),
            &a_tokens
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice2(), Some(issuer1()), None, None, None),
            vec![a_tokens[0].clone()],
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice2(), Some(issuer2()), None, None, None),
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
            ctr.sbt_tokens_by_owner(alice(), None, None, None, None),
            vec![alice_issuer2.clone(), alice_issuer3.clone()]
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer2()), None, None, None),
            vec![alice_issuer2.clone()]
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer3()), None, None, None),
            vec![alice_issuer3.clone()]
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer2()), Some(1), None, None),
            vec![alice_issuer2]
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer2()), Some(4), None, None),
            vec![(issuer2(), vec![mk_owned_token(5, m4_1.clone())])]
        );

        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer1()), Some(5), None, None),
            vec![]
        );

        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer2()), Some(5), None, None),
            vec![]
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer3()), Some(1), None, None),
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
            ctr.sbt_tokens_by_owner(alice(), None, None, None, None),
            vec![alice_issuer2.clone(), alice_issuer3.clone()]
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer2()), None, None, None),
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

        let log1 = mk_log_str("ban", &format!(r#"["{}"]"#, alice()));
        let log2 = mk_log_str(
            "soul_transfer",
            &format!(r#"{{"from":"{}","to":"{}"}}"#, alice(), alice2()),
        );
        assert_eq!(test_utils::get_logs(), vec![log1, log2].concat());
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 0);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer1(), None), 2);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer2(), None), 1);

        assert!(ctr.is_banned(alice()));
        assert!(!ctr.is_banned(alice2()));

        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), None, None, None, None),
            vec![]
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice2(), None, None, None, None),
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
    }

    #[test]
    fn soul_transfer_with_continuation() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 2 * MINT_DEPOSIT);
        // test1: simple case: alice has one token and she owns alice2 account as well. She
        // will do transfer from alice -> alice2
        let m1_1 = mk_metadata(1, Some(START + 10));
        let m2_1 = mk_metadata(2, Some(START + 11));
        let m3_1 = mk_metadata(3, Some(START + 12));
        let m4_1 = mk_metadata(4, Some(START + 13));
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone(), m2_1.clone()])]);

        ctx.predecessor_account_id = issuer2();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), vec![m3_1.clone(), m4_1.clone()])]);

        // make soul transfer
        ctx.predecessor_account_id = alice();
        testing_env!(ctx.clone());
        let mut result = ctr._sbt_soul_transfer(alice2(), 3);
        assert_eq!((3, false), result);
        assert!(test_utils::get_logs().len() == 1);
        result = ctr._sbt_soul_transfer(alice2(), 3);
        assert_eq!((1, true), result);
        assert!(test_utils::get_logs().len() == 2);

        let log_soul_transfer = mk_log_str(
            "soul_transfer",
            &format!(r#"{{"from":"{}","to":"{}"}}"#, alice(), alice2()),
        );
        assert_eq!(test_utils::get_logs()[1], log_soul_transfer[0]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 0);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 0);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer1(), None), 2);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer2(), None), 2);
        assert!(ctr.is_banned(alice()));
        assert!(!ctr.is_banned(alice2()));
    }

    #[test]
    fn soul_transfer_no_tokens_from_caller() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 1 * MINT_DEPOSIT);
        ctx.predecessor_account_id = alice();
        testing_env!(ctx.clone());
        assert!(!ctr.is_banned(alice()));
        assert!(!ctr.is_banned(alice2()));
        ctr.sbt_soul_transfer(alice2(), None);
        assert!(ctr.is_banned(alice()));
        assert!(!ctr.is_banned(alice2()));
        // assert ban even is being emited after the caller with zero tokens has invoked the soul_transfer
        let log_ban = mk_log_str("ban", &format!("[\"{}\"]", alice()));
        assert_eq!(test_utils::get_logs(), log_ban);
    }

    #[test]
    fn soul_transfer_limit() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 150 * MINT_DEPOSIT);
        let batch_metadata = mk_batch_metadata(100);
        assert!(batch_metadata.len() == 100);

        // issuer_1
        ctr.sbt_mint(vec![(alice(), batch_metadata[..50].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 50);

        // issuer_2
        ctx.predecessor_account_id = issuer2();
        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), batch_metadata[50..].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 50);

        // add more tokens to issuer_1
        ctx.predecessor_account_id = issuer1();
        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(bob(), batch_metadata[..20].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(bob(), issuer1(), None), 20);

        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice2(), batch_metadata[..20].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer1(), None), 20);

        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(carol(), batch_metadata[..20].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(carol(), issuer1(), None), 20);

        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(dan(), batch_metadata[..10].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(dan(), issuer1(), None), 10);

        // soul transfer alice->alice2
        ctx.predecessor_account_id = alice();
        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        let limit: u32 = 25; //anything above this limit will fail due to exceeding maximum gas usage per call

        let mut result = ctr._sbt_soul_transfer(alice2(), limit as usize);
        while !result.1 {
            ctx.prepaid_gas = max_gas();
            testing_env!(ctx.clone());
            result = ctr._sbt_soul_transfer(alice2(), limit as usize);
        }

        // check all the balances afterwards
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 0);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 0);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer1(), None), 70);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer2(), None), 50);
        assert_eq!(ctr.sbt_supply_by_owner(bob(), issuer1(), None), 20);
        assert_eq!(ctr.sbt_supply_by_owner(carol(), issuer1(), None), 20);
        assert_eq!(ctr.sbt_supply_by_owner(dan(), issuer1(), None), 10);
    }

    #[test]
    #[should_panic(expected = "HostError(GasLimitExceeded)")]
    fn soul_transfer_exceeded_limit() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 150 * MINT_DEPOSIT);
        let batch_metadata = mk_batch_metadata(100);
        assert!(batch_metadata.len() == 100);

        // issuer_1
        ctr.sbt_mint(vec![(alice(), batch_metadata[..50].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 50);

        // issuer_2
        ctx.predecessor_account_id = issuer2();
        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), batch_metadata[50..].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 50);

        // add more tokens to issuer_1
        ctx.predecessor_account_id = issuer1();
        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(bob(), batch_metadata[..20].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(bob(), issuer1(), None), 20);

        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice2(), batch_metadata[..20].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer1(), None), 20);

        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(carol(), batch_metadata[..20].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(carol(), issuer1(), None), 20);

        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(dan(), batch_metadata[..10].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(dan(), issuer1(), None), 10);

        // soul transfer alice->alice2
        ctx.predecessor_account_id = alice();
        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        let limit: u32 = 30; //anything above this limit will fail due to exceeding maximum gas usage per call

        let mut result = ctr._sbt_soul_transfer(alice2(), limit as usize);
        while !result.1 {
            ctx.prepaid_gas = max_gas();
            testing_env!(ctx.clone());
            result = ctr._sbt_soul_transfer(alice2(), limit as usize);
        }
    }

    #[test]
    fn soul_transfer_limit_basics() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 60 * MINT_DEPOSIT);
        let batch_metadata = mk_batch_metadata(40);
        assert!(batch_metadata.len() == 40);

        // issuer_1
        ctr.sbt_mint(vec![(alice(), batch_metadata[..20].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 20);

        // issuer_2
        ctx.predecessor_account_id = issuer2();
        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), batch_metadata[20..].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 20);

        ctx.predecessor_account_id = alice();
        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());

        let limit: u32 = 10;
        let mut result = ctr._sbt_soul_transfer(alice2(), limit as usize);
        assert_eq!((limit, false), result);

        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        result = ctr._sbt_soul_transfer(alice2(), limit as usize);
        assert_eq!((limit, false), result);

        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        result = ctr._sbt_soul_transfer(alice2(), limit as usize);
        assert_eq!((limit, false), result);

        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        result = ctr._sbt_soul_transfer(alice2(), limit as usize);
        assert_eq!((limit, false), result);

        // resumed transfer but no more tokens to transfer
        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        result = ctr._sbt_soul_transfer(alice2(), limit as usize);
        assert_eq!((0, true), result);

        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 0);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 0);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer1(), None), 20);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer2(), None), 20);
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

    #[test]
    fn registry_renew_one_issuer() {
        let (_, mut ctr) = setup(&issuer1(), 3 * MINT_DEPOSIT);

        // mint two tokens
        let m1_1 = mk_metadata(1, Some(START + 10));
        let m2_1 = mk_metadata(2, Some(START + 11));
        let tokens = ctr.sbt_mint(vec![(alice(), vec![m1_1.clone(), m2_1.clone()])]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 2);

        // renvew the two tokens
        let new_expire = START + 100;
        ctr.sbt_renew(tokens, new_expire);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 2);
        let m1_1_renewed = mk_metadata(1, Some(new_expire));
        let m2_1_renewed = mk_metadata(2, Some(new_expire));

        // assert the two tokens have been renewed (new expire_at)
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer1()), None, None, None),
            vec![(
                issuer1(),
                vec![
                    mk_owned_token(1, m1_1_renewed.clone()),
                    mk_owned_token(2, m2_1_renewed.clone())
                ]
            ),]
        );
    }

    #[test]
    fn registry_renew_multiple_issuers() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 3 * MINT_DEPOSIT);

        // mint two tokens by issuer1
        let m1_1 = mk_metadata(1, Some(START + 10));
        let m2_1 = mk_metadata(2, Some(START + 11));
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone(), m2_1.clone()])]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 2);

        // mint two tokens by issuer2
        let m1_2 = mk_metadata(1, Some(START + 10));
        let m2_2: TokenMetadata = mk_metadata(2, Some(START + 12));
        ctx.predecessor_account_id = issuer2();
        testing_env!(ctx.clone());
        let tokens_issuer2 = ctr.sbt_mint(vec![(alice(), vec![m1_2.clone(), m2_2.clone()])]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 2);

        // renvew the two tokens
        ctr.sbt_renew(tokens_issuer2, START + 100);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 2);
        let m1_2_renewed = mk_metadata(1, Some(START + 100));
        let m2_2_renewed = mk_metadata(2, Some(START + 100));

        // assert tokens issued by issuer2 has been renewed (new expire_at)
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer2()), None, None, None),
            vec![(
                issuer2(),
                vec![
                    mk_owned_token(1, m1_2_renewed.clone()),
                    mk_owned_token(2, m2_2_renewed.clone())
                ]
            ),]
        );

        // assert tokens issued by issuer1 has not been renewed (new expire_at)
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer1()), None, None, None),
            vec![(
                issuer1(),
                vec![
                    mk_owned_token(1, m1_1.clone()),
                    mk_owned_token(2, m2_1.clone())
                ]
            ),]
        );
    }

    #[test]
    #[should_panic]
    fn registry_renew_basics() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 3 * MINT_DEPOSIT);

        // mint two tokens
        let m1_1 = mk_metadata(1, Some(START + 10));
        let tokens = ctr.sbt_mint(vec![(alice(), vec![m1_1.clone()])]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 1);

        // check if only the issuer can renew the tokens (should panic)
        ctx.predecessor_account_id = issuer2();
        testing_env!(ctx.clone());
        ctr.sbt_renew(tokens, START + 100);
    }

    #[test]
    fn registry_renew_event() {
        let (_, mut ctr) = setup(&issuer1(), 3 * MINT_DEPOSIT);

        // mint two tokens
        let m1_1 = mk_metadata(1, Some(START + 10));
        let tokens = ctr.sbt_mint(vec![(alice(), vec![m1_1.clone()])]);
        ctr.sbt_renew(tokens.clone(), START + 100);
        let log_mint = mk_log_str(
            "mint",
            &format!(
                r#"{{"issuer":"{}","tokens":[["{}",[1]]]}}"#,
                issuer1(),
                alice()
            ),
        );
        let log_renew = mk_log_str(
            "renew",
            &format!(r#"{{"issuer":"{}","tokens":[{}]}}"#, issuer1(), tokens[0]),
        );
        assert_eq!(test_utils::get_logs(), vec![log_mint, log_renew].concat());
    }

    #[test]
    fn sbt_recover_basics() {
        let (mut ctx, mut ctr) = setup(&issuer2(), 3 * MINT_DEPOSIT);
        let m1_1 = mk_metadata(1, Some(START + 10));
        let m2_1 = mk_metadata(2, Some(START + 10));
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone()])]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 1);

        //issue tokens by a different issuer
        ctx.predecessor_account_id = issuer1();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone(), m2_1.clone()])]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 2);

        ctr.sbt_recover(alice(), bob());
        let recover_log = mk_log_str(
            "recover",
            &format!(
                r#"{{"issuer":"{}","old_owner":"{}","new_owner":"{}"}}"#,
                issuer1(),
                alice(),
                bob()
            ),
        );
        assert_eq!(test_utils::get_logs().len(), 2);
        assert_eq!(test_utils::get_logs()[1], recover_log[0]);
        assert!(!ctr.is_banned(alice()));
        assert!(!ctr.is_banned(bob()));
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 0);
        assert_eq!(ctr.sbt_supply_by_owner(bob(), issuer1(), None), 2);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 1); //check if alice still holds the tokens issued by a different issuer
        assert_eq!(
            ctr.sbt_tokens_by_owner(bob(), Some(issuer1()), None, None, None),
            vec![(
                issuer1(),
                vec![
                    mk_owned_token(1, m1_1.clone()),
                    mk_owned_token(2, m2_1.clone())
                ]
            ),]
        );
        assert_eq!(ctr.sbt_supply(issuer1()), 2);
        assert_eq!(ctr.sbt_supply(issuer2()), 1);
        assert_eq!(ctr.sbt_supply_by_class(issuer2(), 1), 1);
        assert_eq!(ctr.sbt_supply_by_class(issuer1(), 1), 1);
        assert_eq!(ctr.sbt_supply_by_class(issuer1(), 2), 1);

        assert_eq!(
            ctr.sbt_tokens(issuer1(), None, None),
            vec![
                mk_token(1, bob(), m1_1.clone()),
                mk_token(2, bob(), m2_1.clone())
            ]
        );
        assert_eq!(
            ctr.sbt(issuer1(), 1).unwrap(),
            mk_token(1, bob(), m1_1.clone())
        );
        assert_eq!(
            ctr.sbt(issuer1(), 2).unwrap(),
            mk_token(2, bob(), m2_1.clone())
        );
        assert_eq!(
            ctr.sbt(issuer2(), 1).unwrap(),
            mk_token(1, alice(), m1_1.clone())
        );
    }

    #[test]
    #[should_panic(expected = "not enough NEAR storage depost")]
    fn sbt_recover_growing_storage_desposit_fail() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 2 * MINT_DEPOSIT);
        let m1_1 = mk_metadata(1, Some(START + 10));
        let m1_2 = mk_metadata(2, Some(START + 10));
        let m1_3 = mk_metadata(3, Some(START + 10));
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone()])]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 1);

        ctx.predecessor_account_id = issuer2();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(
            alice(),
            vec![m1_1.clone(), m1_2.clone(), m1_3.clone()],
        )]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 3);

        //set attached deposit to zero, should fail since the storage grows and we do not cover it
        ctx.attached_deposit = 0;
        testing_env!(ctx.clone());
        ctr._sbt_recover(alice(), bob(), 1);
    }

    #[test]
    fn sbt_recover_growing_storage_desposit_pass() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 2 * MINT_DEPOSIT);
        let m1_1 = mk_metadata(1, Some(START + 10));
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone()])]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 1);

        ctx.predecessor_account_id = issuer2();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone()])]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 1);

        // storage will grow so need to attach deposit.
        ctx.attached_deposit = MINT_DEPOSIT;
        testing_env!(ctx.clone());
        ctr.sbt_recover(alice(), bob());
        assert_eq!(ctr.sbt_supply_by_owner(bob(), issuer2(), None), 1);
    }

    #[test]
    fn sbt_recover_with_continuation_basics() {
        let (_, mut ctr) = setup(&issuer1(), 5 * MINT_DEPOSIT);
        let m1_1 = mk_metadata(1, Some(START + 10));
        let m2_1 = mk_metadata(2, Some(START + 11));
        let m3_1 = mk_metadata(3, Some(START + 12));
        let m4_1 = mk_metadata(4, Some(START + 13));
        ctr.sbt_mint(vec![(
            alice(),
            vec![m1_1.clone(), m2_1.clone(), m3_1.clone(), m4_1.clone()],
        )]);

        // sbt_recover
        let mut result = ctr._sbt_recover(alice(), alice2(), 3);
        assert_eq!((3, false), result);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer1(), None), 3);
        assert!(test_utils::get_logs().len() == 1);
        result = ctr._sbt_recover(alice(), alice2(), 3);
        assert_eq!((1, true), result);
        assert!(test_utils::get_logs().len() == 2);

        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 0);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer1(), None), 4);
    }

    #[test]
    fn sbt_revoke() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 2 * MINT_DEPOSIT);

        let m1_1 = mk_metadata(1, Some(START + 10));
        let m2_1 = mk_metadata(2, Some(START + 11));
        let m3_1 = mk_metadata(3, Some(START + 21));

        let current_timestamp = ctx.block_timestamp;

        let m1_1_revoked = mk_metadata(1, Some(current_timestamp));
        let m2_1_revoked = mk_metadata(2, Some(current_timestamp));
        let m3_1_revoked = mk_metadata(3, Some(current_timestamp));

        let tokens_issuer_1 = ctr.sbt_mint(vec![(
            alice(),
            vec![m1_1.clone(), m2_1.clone(), m3_1.clone()],
        )]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 3);

        //issue tokens by a different issuer
        ctx.predecessor_account_id = issuer2();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(bob(), vec![m1_1.clone(), m2_1.clone()])]);
        ctr.sbt_mint(vec![(alice(), vec![m3_1.clone()])]);
        assert_eq!(ctr.sbt_supply_by_owner(bob(), issuer2(), None), 2);

        //revoke tokens issued by issuer1
        ctx.predecessor_account_id = issuer1();
        testing_env!(ctx.clone());
        ctr.sbt_revoke(tokens_issuer_1, false);

        let log_revoke = mk_log_str(
            "revoke",
            &format!(r#"{{"issuer":"{}","tokens":[1,2,3]}}"#, issuer1()),
        );
        assert_eq!(test_utils::get_logs().len(), 1);
        assert_eq!(test_utils::get_logs()[0], log_revoke[0]);

        assert_eq!(ctr.sbt_supply(issuer1()), 3);
        assert_eq!(ctr.sbt_supply(issuer2()), 3);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 3);
        assert_eq!(ctr.sbt_supply_by_owner(bob(), issuer2(), None), 2);
        assert_eq!(
            ctr.sbt_tokens_by_owner(alice(), Some(issuer1()), None, None, None),
            vec![(
                issuer1(),
                vec![
                    mk_owned_token(1, m1_1_revoked.clone()),
                    mk_owned_token(2, m2_1_revoked.clone()),
                    mk_owned_token(3, m3_1_revoked.clone()),
                ]
            ),]
        );
        assert_eq!(
            ctr.sbt_tokens(issuer1(), None, None),
            vec![
                mk_token(1, alice(), m1_1_revoked.clone()),
                mk_token(2, alice(), m2_1_revoked.clone()),
                mk_token(3, alice(), m3_1_revoked.clone())
            ]
        );
        assert_eq!(
            ctr.sbt_tokens(issuer2(), None, None),
            vec![
                mk_token(1, bob(), m1_1.clone()),
                mk_token(2, bob(), m2_1.clone()),
                mk_token(3, alice(), m3_1.clone())
            ]
        )
    }

    #[test]
    fn sbt_revoke_burn() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 2 * MINT_DEPOSIT);

        let m1_1 = mk_metadata(1, Some(START + 10));
        let m2_1 = mk_metadata(2, Some(START + 11));
        let m3_1 = mk_metadata(3, Some(START + 21));

        let tokens_to_burn = ctr.sbt_mint(vec![
            (alice(), vec![m1_1.clone(), m2_1.clone()]),
            (bob(), vec![m1_1.clone()]),
        ]);

        ctr.sbt_mint(vec![(alice(), vec![m3_1.clone()])]);

        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 3);
        assert_eq!(ctr.sbt_supply_by_owner(bob(), issuer1(), None), 1);

        //issue tokens by a different issuer
        ctx.predecessor_account_id = issuer2();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(bob(), vec![m1_1.clone(), m2_1.clone()])]);
        ctr.sbt_mint(vec![(alice(), vec![m3_1.clone()])]);
        assert_eq!(ctr.sbt_supply_by_owner(bob(), issuer2(), None), 2);

        //revoke tokens issued by issuer1
        ctx.predecessor_account_id = issuer1();
        testing_env!(ctx.clone());
        ctr.sbt_revoke(tokens_to_burn, true);

        let log_burn = mk_log_str(
            "burn",
            &format!(r#"{{"issuer":"{}","tokens":[1,2,3]}}"#, issuer1()),
        );
        assert_eq!(test_utils::get_logs().len(), 1);
        assert_eq!(test_utils::get_logs()[0], log_burn[0]);
        assert_eq!(ctr.sbt_supply(issuer1()), 1);
        assert_eq!(ctr.sbt_supply(issuer2()), 3);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer1(), None), 1);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 1);
        assert_eq!(ctr.sbt_supply_by_owner(bob(), issuer2(), None), 2);
        assert_eq!(ctr.sbt_supply_by_class(issuer1(), 1), 0);
        assert_eq!(ctr.sbt_supply_by_class(issuer1(), 2), 0);
        assert_eq!(ctr.sbt_supply_by_class(issuer1(), 3), 1);
        assert_eq!(ctr.sbt_supply_by_class(issuer2(), 1), 1);
        assert_eq!(ctr.sbt_supply_by_class(issuer2(), 2), 1);
        assert_eq!(ctr.sbt_supply_by_class(issuer2(), 3), 1);

        assert_eq!(
            ctr.sbt_tokens(issuer1(), None, None),
            vec![mk_token(4, alice(), m3_1.clone())],
        );
        assert_eq!(
            ctr.sbt_tokens(issuer2(), None, None),
            vec![
                mk_token(1, bob(), m1_1.clone()),
                mk_token(2, bob(), m2_1.clone()),
                mk_token(3, alice(), m3_1.clone())
            ]
        )
    }

    // sbt_ban
    #[test]
    fn sbt_soul_transfer_ban() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 2 * MINT_DEPOSIT);
        let m1_1 = mk_metadata(1, Some(START + 10));
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone()])]);
        assert!(!ctr.is_banned(alice()));

        ctx.predecessor_account_id = alice();
        testing_env!(ctx.clone());
        ctr.sbt_soul_transfer(alice2(), None);

        assert!(ctr.is_banned(alice()));
        assert!(!ctr.is_banned(alice2()));
    }

    #[test]
    fn sbt_recover_limit() {
        let (mut ctx, mut ctr) = setup(&issuer2(), 150 * MINT_DEPOSIT);
        let batch_metadata = mk_batch_metadata(100);
        assert!(batch_metadata.len() == 100);

        // issuer_2
        ctr.sbt_mint(vec![(alice(), batch_metadata[..50].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 50);

        // // add more tokens to issuer_2
        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), batch_metadata[50..].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 100);

        // add more tokens to issuer_1
        ctx.predecessor_account_id = issuer1();
        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(bob(), batch_metadata[..20].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(bob(), issuer1(), None), 20);

        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice2(), batch_metadata[..20].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer1(), None), 20);

        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(carol(), batch_metadata[..20].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(carol(), issuer1(), None), 20);

        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(dan(), batch_metadata[..10].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(dan(), issuer1(), None), 10);

        // sbt_recover alice->alice2
        ctx.predecessor_account_id = issuer2();
        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        let limit: u32 = 20; //anything above this limit will fail due to exceeding maximum gas usage per call

        let mut result = ctr._sbt_recover(alice(), alice2(), limit as usize);
        while !result.1 {
            ctx.prepaid_gas = max_gas();
            testing_env!(ctx.clone());
            result = ctr._sbt_recover(alice(), alice2(), limit as usize);
        }

        // check all the balances afterwards
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 0);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer2(), None), 100);
    }

    #[test]
    #[should_panic(expected = "HostError(GasLimitExceeded)")]
    fn sbt_recover_limit_exceeded() {
        let (mut ctx, mut ctr) = setup(&issuer2(), 150 * MINT_DEPOSIT);
        let batch_metadata = mk_batch_metadata(100);
        assert!(batch_metadata.len() == 100);

        // issuer_2
        ctr.sbt_mint(vec![(alice(), batch_metadata[..50].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 50);

        // // add more tokens to issuer_2
        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), batch_metadata[50..].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 100);

        // add more tokens to issuer_1
        ctx.predecessor_account_id = issuer1();
        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(bob(), batch_metadata[..20].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(bob(), issuer1(), None), 20);

        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice2(), batch_metadata[..20].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer1(), None), 20);

        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(carol(), batch_metadata[..20].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(carol(), issuer1(), None), 20);

        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(dan(), batch_metadata[..10].to_vec())]);
        assert_eq!(ctr.sbt_supply_by_owner(dan(), issuer1(), None), 10);

        // sbt_recover alice->alice2
        ctx.predecessor_account_id = issuer2();
        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        let limit: u32 = 25; // this value exceedes the gas limit allowed per call and should fail

        let mut result = ctr._sbt_recover(alice(), alice2(), limit as usize);
        while !result.1 {
            ctx.prepaid_gas = max_gas();
            testing_env!(ctx.clone());
            result = ctr._sbt_recover(alice(), alice2(), limit as usize);
        }

        // check all the balances afterwards
        assert_eq!(ctr.sbt_supply_by_owner(alice(), issuer2(), None), 0);
        assert_eq!(ctr.sbt_supply_by_owner(alice2(), issuer2(), None), 100);
    }

    #[test]
    #[should_panic(expected = "from account is banned. Cannot start the transfer")]
    fn sbt_soul_transfer_from_banned_account() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 1 * MINT_DEPOSIT);
        let m1_1 = mk_metadata(1, Some(START + 10));
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone()])]);
        assert!(!ctr.is_banned(alice()));

        // ban the from account
        ctr.banlist.insert(&alice());
        assert!(ctr.is_banned(alice()));

        ctx.predecessor_account_id = alice();
        testing_env!(ctx.clone());
        ctr.sbt_soul_transfer(alice2(), None);
    }

    #[test]
    #[should_panic(expected = "receiver account is banned. Cannot start the transfer")]
    fn sbt_soul_transfer_to_banned_account() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 1 * MINT_DEPOSIT);
        let m1_1 = mk_metadata(1, Some(START + 10));
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone()])]);
        assert!(!ctr.is_banned(alice()));

        // ban the reciver account
        ctr.banlist.insert(&alice2());
        assert!(ctr.is_banned(alice2()));

        ctx.predecessor_account_id = alice();
        testing_env!(ctx.clone());
        ctr.sbt_soul_transfer(alice2(), None);
    }

    #[test]
    fn sbt_soul_transfer_ban_with_continuation() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 50 * MINT_DEPOSIT);
        let batch_metadata = mk_batch_metadata(50);
        ctr.sbt_mint(vec![(alice(), batch_metadata)]);
        assert!(!ctr.is_banned(alice()));

        ctx.predecessor_account_id = alice();
        testing_env!(ctx.clone());
        // soul transfer
        let result: (u32, bool) = ctr.sbt_soul_transfer(alice2(), None);
        assert!(result.1 == false);

        // assert the from account is banned after the first soul transfer execution
        assert!(ctr.is_banned(alice()));
        assert!(!ctr.is_banned(alice2()));

        ctx.prepaid_gas = max_gas();
        testing_env!(ctx.clone());
        ctr.sbt_soul_transfer(alice2(), None);
        let result: (u32, bool) = ctr.sbt_soul_transfer(alice2(), None);
        assert!(result.1 == true);

        // assert it stays banned after the soul transfer has been completed
        assert!(ctr.is_banned(alice()));
        assert!(!ctr.is_banned(alice2()));
    }

    #[test]
    fn sbt_recover_ban() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 2 * MINT_DEPOSIT);
        let m1_1 = mk_metadata(1, Some(START + 10));
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone()])]);
        assert!(!ctr.is_banned(alice()));

        ctx.predecessor_account_id = issuer1();
        testing_env!(ctx.clone());
        ctr.sbt_recover(alice(), alice2());
        // sbt_recover should not ban the source account
        assert!(!ctr.is_banned(alice()));
        assert!(!ctr.is_banned(alice2()));
    }

    #[test]
    #[should_panic(expected = "account alice.near is banned")]
    fn sbt_mint_to_banned_account() {
        let (_, mut ctr) = setup(&issuer1(), 2 * MINT_DEPOSIT);
        let m1_1 = mk_metadata(1, Some(START + 10));

        //ban alice account
        ctr.banlist.insert(&alice());
        assert!(ctr.is_banned(alice()));

        //try to mint to a banned account
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone()])]);
    }

    #[test]
    fn sbt_tokens_by_owner_non_expired() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 4 * MINT_DEPOSIT);
        let m1_1 = mk_metadata(1, Some(START));
        let m1_2 = mk_metadata(2, Some(START));
        let m1_3 = mk_metadata(3, Some(START + 100));
        let m1_4 = mk_metadata(4, Some(START + 100));
        ctr.sbt_mint(vec![(
            alice(),
            vec![m1_1.clone(), m1_2.clone(), m1_3.clone(), m1_4.clone()],
        )]);

        let res = ctr.sbt_tokens_by_owner(alice(), None, None, None, Some(true));
        assert_eq!(res[0].1.len(), 4);

        // fast forward so the first two sbts are expired
        ctx.block_timestamp = START + 50;
        testing_env!(ctx.clone());

        let res = ctr.sbt_tokens_by_owner(alice(), None, None, None, Some(true));
        assert_eq!(res[0].1.len(), 2);
        let res = ctr.sbt_tokens_by_owner(alice(), None, None, None, None);
        assert_eq!(res[0].1.len(), 4);
    }
}
