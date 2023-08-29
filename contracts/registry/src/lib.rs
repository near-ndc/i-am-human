use std::collections::{HashMap, HashSet};

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, LookupMap, TreeMap, UnorderedMap, UnorderedSet};
use near_sdk::serde_json::value::RawValue;
use near_sdk::{env, near_bindgen, require, serde_json, AccountId, Gas, PanicOnDefault, Promise};

use cost::MILI_NEAR;
use sbt::*;

use crate::storage::*;

pub mod events;
pub mod migrate;
pub mod registry;
pub mod storage;

const IS_HUMAN_GAS: Gas = Gas(12 * Gas::ONE_TERA.0);

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// Registry admin, expected to be a DAO.
    pub authority: AccountId,

    /// registry of approved SBT contracts to issue tokens
    pub sbt_issuers: UnorderedMap<AccountId, IssuerId>,
    pub issuer_id_map: LookupMap<IssuerId, AccountId>, // reverse index
    /// store ongoing soul transfers by "old owner"
    pub(crate) ongoing_soul_tx: LookupMap<AccountId, IssuerTokenId>,

    /// registry of banned accounts created through `Nep393Event::Ban` (eg: soul transfer).
    pub(crate) banlist: UnorderedSet<AccountId>,
    /// Map of accounts that are marked by a committee to have a special status (eg: blacklist,
    /// whitelist).
    pub(crate) flagged: LookupMap<AccountId, AccountFlag>,
    /// list of admins that can manage flagged accounts map.
    pub(crate) authorized_flaggers: LazyOption<Vec<AccountId>>,

    pub(crate) supply_by_owner: LookupMap<(AccountId, IssuerId), u64>,
    pub(crate) supply_by_class: LookupMap<(IssuerId, ClassId), u64>,
    pub(crate) supply_by_issuer: LookupMap<IssuerId, u64>,

    /// maps user balance key to tokenID
    pub(crate) balances: TreeMap<BalanceKey, TokenId>,
    pub(crate) issuer_tokens: LookupMap<IssuerTokenId, TokenData>,

    /// map of SBT contract -> next available token_id
    pub(crate) next_token_ids: LookupMap<IssuerId, TokenId>,
    pub(crate) next_issuer_id: IssuerId,

    /// tuple of (required issuer, [required list of classes]) that represents mandatory
    /// requirements to be verified as human for `is_human` and `is_human_call` methods.
    pub(crate) iah_sbts: (AccountId, Vec<ClassId>),
    /// list of admins allowed to mint tokens 
    pub(crate) admins: Vec<AccountId>,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    /// Contract constructor.
    /// `iah_issuer`: required issuer for is_human check.
    /// `iah_classes`: required list of classes for is_human check.
    #[init]
    pub fn new(
        authority: AccountId,
        iah_issuer: AccountId,
        iah_classes: Vec<ClassId>,
        authorized_flaggers: Vec<AccountId>,
    ) -> Self {
        require!(
            iah_classes.len() > 0,
            "iah_classes must be a non empty list"
        );
        let mut contract = Self {
            authority: authority,
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
            iah_sbts: (iah_issuer.clone(), iah_classes),
            flagged: LookupMap::new(StorageKey::Flagged),
            authorized_flaggers: LazyOption::new(
                StorageKey::AdminsFlagged,
                Some(&authorized_flaggers),
            ),
            admins: vec![]
        };
        contract._add_sbt_issuer(&iah_issuer);
        contract
    }

    //
    // Queries
    //

    pub fn sbt_issuers(&self) -> Vec<AccountId> {
        self.sbt_issuers.keys().collect()
    }

    /// Returns IAH class set: required token classes to be approved as a human by the
    /// `is_human`.
    pub fn iah_class_set(&self) -> ClassSet {
        vec![self.iah_sbts.clone()]
    }

    #[inline]
    fn _is_banned(&self, account: &AccountId) -> bool {
        self.banlist.contains(account)
    }

    /// Returns account status if it was flagged. Returns None if the account was not flagged.
    pub fn account_flagged(&self, account: AccountId) -> Option<AccountFlag> {
        self.flagged.get(&account)
    }

    /// Returns empty list if the account is NOT a human according to the IAH protocol.
    /// Otherwise returns list of SBTs (identifed by issuer and list of token IDs) proving
    /// the `account` humanity.
    pub fn is_human(&self, account: AccountId) -> SBTs {
        self._is_human(&account)
    }

    fn _is_human(&self, account: &AccountId) -> SBTs {
        if self.flagged.get(&account) == Some(AccountFlag::Blacklisted) || self._is_banned(&account)
        {
            return vec![];
        }
        let issuer = Some(self.iah_sbts.0.clone());
        let mut proof: Vec<TokenId> = Vec::new();
        // check if user has tokens from all classes
        for cls in &self.iah_sbts.1 {
            let tokens = self.sbt_tokens_by_owner(
                account.clone(),
                issuer.clone(),
                Some(*cls),
                Some(1),
                None,
            );
            // we need to check class, because the query can return a "next" token if a user
            // doesn't have the token of requested class.
            if !(tokens.len() > 0 && tokens[0].1[0].metadata.class == *cls) {
                return vec![];
            }
            proof.push(tokens[0].1[0].token)
        }
        vec![(self.iah_sbts.0.clone(), proof)]
    }

    pub fn get_authority(self) -> AccountId {
        self.authority
    }

    //
    // Transactions
    //

    /// sbt_mint_iah is a wrapper around `sbt_mint` and `is_human`. It mints SBTs only when
    /// all recipients are humans. Panics if one of the recipients is not a human.
    #[payable]
    pub fn sbt_mint_iah(
        &mut self,
        token_spec: Vec<(AccountId, Vec<TokenMetadata>)>,
    ) -> Vec<TokenId> {
        let issuer = &env::predecessor_account_id();
        for ts in &token_spec {
            require!(
                !self._is_human(&ts.0).is_empty(),
                format!("{} is not a human", &ts.0)
            );
        }
        self._sbt_mint(issuer, token_spec)
    }

    /// Transfers atomically all SBT tokens from one account to another account.
    /// The caller must be an SBT holder and the `recipient` must not be a banned account.
    /// Returns the amount of tokens transferred and a boolean: `true` if the whole
    /// process has finished, `false` when the process has not finished and should be
    /// continued by a subsequent call.
    /// Emits `Ban` event for the caller at the beginning of the process.
    /// Emits `SoulTransfer` event only once all the tokens from the caller were transferred
    /// and at least one token was transferred (caller had at least 1 sbt).
    /// + User must keep calling the `sbt_soul_transfer` until `true` is returned.
    /// + If caller does not have any tokens, nothing will be transfered, the caller
    ///   will be banned and `Ban` event will be emitted.
    // Transfers the account flag from the owner to the recipient.
    // Fails if there is a potential conflict between the caller's and recipient's flags,
    // specifically when one account is `Blacklisted` and the other is `Verified`.
    #[payable]
    pub fn sbt_soul_transfer(
        &mut self,
        recipient: AccountId,
        #[allow(unused_variables)] memo: Option<String>,
    ) -> (u32, bool) {
        // TODO: test what is the max safe amount of updates
        self._sbt_soul_transfer(recipient, 25)
    }

    pub(crate) fn _transfer_flag(&mut self, from: &AccountId, recipient: &AccountId) {
        if let Some(flag_from) = self.flagged.get(from) {
            match self.flagged.get(recipient) {
                Some(AccountFlag::Verified) => require!(
                    flag_from != AccountFlag::Blacklisted,
                    "can't transfer soul from a blacklisted account to a verified account"
                ),
                Some(AccountFlag::Blacklisted) => require!(
                    flag_from != AccountFlag::Verified,
                    "can't transfer soul from a verified account to a blacklisted account"
                ),
                None => {
                    self.flagged.insert(recipient, &flag_from);
                }
            }
        }
    }

    // execution of the sbt_soul_transfer in this function to parametrize `max_updates` in
    // order to facilitate tests.
    pub(crate) fn _sbt_soul_transfer(&mut self, recipient: AccountId, limit: usize) -> (u32, bool) {
        let owner = env::predecessor_account_id();

        let (resumed, start) = self.transfer_continuation(&owner, &recipient, true);
        if !resumed {
            self._transfer_flag(&owner, &recipient);
        }

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

    /// Checks if the `predecessor_account_id` is human. If yes, then calls:
    ///
    ///    ctr.function({caller: predecessor_account_id(),
    ///                 iah_proof: SBTs,
    ///                 payload: payload})
    ///
    /// `payload` must be a JSON string, and it will be passed through the default interface,
    /// hence it will be JSON deserialized when using SDK.
    /// Panics if the predecessor is not a human.
    #[payable]
    pub fn is_human_call(&mut self, ctr: AccountId, function: String, payload: String) -> Promise {
        let caller = env::predecessor_account_id();
        let iah_proof = self._is_human(&caller);
        require!(!iah_proof.is_empty(), "caller not a human");

        let args = IsHumanCallbackArgs {
            caller,
            iah_proof,
            payload: &RawValue::from_string(payload).unwrap(),
        };
        Promise::new(ctr).function_call(
            function,
            serde_json::to_vec(&args).unwrap(),
            env::attached_deposit(),
            env::prepaid_gas() - IS_HUMAN_GAS,
        )
    }

    // NOTE: we are using IssuerTokenId to return Issuer and ClassId. This works as expected
    // and doesn't create API conflict because this is a crate private function. The reason we
    // do it is to avoid another struct creation and save the bytes.
    pub(crate) fn start_transfer_with_continuation(
        &mut self,
        owner: &AccountId,
        recipient: &AccountId,
        ban_owner: bool,
    ) -> IssuerTokenId {
        self.assert_not_banned(recipient);
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

    /// Method to burn all caller tokens (from all issuers).
    /// The method must be called repeatedly until true is returned.
    /// Not all tokens may be burned in a single call due to the gas limitation - in that case
    /// `false` is returned.
    /// The burn event is emitted for all the tokens burned.
    pub fn sbt_burn_all(&mut self) -> bool {
        self._sbt_burn_all(25)
    }

    /// Allows user to burn any of his tokens.
    /// The burn event is emitted for all  tokens burned.
    /// Panics if user has ongoing soul transfer or ongoing recovery or doesn't own a listed
    /// token.
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
        self._add_sbt_issuer(&issuer)
    }

    pub fn change_admin(&mut self, new_admin: AccountId) {
        self.assert_authority();
        self.authority = new_admin;
    }

    pub fn admin_set_authorized_flaggers(&mut self, authorized_flaggers: Vec<AccountId>) {
        self.assert_authority();
        self.authorized_flaggers.set(&authorized_flaggers);
    }

    /// flag accounts
    pub fn admin_flag_accounts(
        &mut self,
        flag: AccountFlag,
        accounts: Vec<AccountId>,
        #[allow(unused_variables)] memo: String,
    ) {
        self.assert_authorized_flagger();
        for a in &accounts {
            self.assert_not_banned(&a);
            self.flagged.insert(a, &flag);
        }
        events::emit_iah_flag_accounts(flag, accounts);
    }

    /// removes flag from the provided account list.
    /// Panics if an account is not currently flagged.
    pub fn admin_unflag_accounts(
        &mut self,
        accounts: Vec<AccountId>,
        #[allow(unused_variables)] memo: String,
    ) {
        self.assert_authorized_flagger();
        for a in &accounts {
            require!(self.flagged.remove(a).is_some());
        }
        events::emit_iah_unflag_accounts(accounts);
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
    pub(crate) fn assert_authorized_flagger(&self) {
        let caller = env::predecessor_account_id();
        let a = self.authorized_flaggers.get();
        if a.is_none() || !a.unwrap().contains(&caller) {
            env::panic_str("not authorized");
        }
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

    fn _add_sbt_issuer(&mut self, issuer: &AccountId) -> bool {
        if self.sbt_issuers.get(issuer).is_some() {
            return false;
        }
        self.sbt_issuers.insert(issuer, &self.next_issuer_id);
        self.issuer_id_map.insert(&self.next_issuer_id, issuer);
        self.next_issuer_id += 1;
        true
    }

    fn _sbt_renew(&mut self, issuer: AccountId, tokens: Vec<TokenId>, expires_at: u64) {
        let issuer_id = self.assert_issuer(&issuer);
        for token in &tokens {
            let token = *token;
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

    fn _sbt_mint(
        &mut self,
        issuer: &AccountId,
        token_spec: Vec<(AccountId, Vec<TokenMetadata>)>,
    ) -> Vec<TokenId> {
        let storage_start = env::storage_usage();
        let storage_deposit = env::attached_deposit();
        require!(
            storage_deposit >= 9 * MILI_NEAR,
            "min required storage deposit: 0.009 NEAR"
        );

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
                require!(metadata.class > 0, "Class must be > 0");
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
                    Some(s) => *s += 1,
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
            issuer,
            tokens: minted,
        }
        .emit();

        let required_deposit =
            (env::storage_usage() - storage_start) as u128 * env::storage_byte_cost();
        require!(
            storage_deposit >= required_deposit,
            format!(
                "not enough NEAR storage deposit, required: {}",
                required_deposit
            )
        );
        if env::current_account_id().as_str().ends_with("testnet") {
            env::log_str(&format!("required deposit: {}", required_deposit));
        }

        ret_token_ids
    }

    /// Method to help parametrize the sbt_burn_all.
    /// limit indicates the number of tokens that will be burned in one call
    pub(crate) fn _sbt_burn_all(&mut self, limit: u32) -> bool {
        let owner = env::predecessor_account_id();
        require!(
            !self.ongoing_soul_tx.contains_key(&owner),
            "can't burn tokens while in soul_transfer"
        );
        let mut tokens_burned: u32 = 0;

        let issuer_token_pair_vec =
            self.sbt_tokens_by_owner(owner.clone(), None, None, Some(limit), Some(true));
        for (issuer, tokens) in issuer_token_pair_vec.iter() {
            let mut token_ids = Vec::new();
            let issuer_id = self.assert_issuer(issuer);
            let mut tokens_burned_per_issuer: u64 = 0;
            for t in tokens.iter() {
                token_ids.push(t.token);
                self.issuer_tokens.remove(&IssuerTokenId {
                    issuer_id,
                    token: t.token,
                });
                let class_id = t.metadata.class;
                self.balances
                    .remove(&balance_key(owner.clone(), issuer_id, class_id));

                // update supply by class
                let key = (issuer_id, class_id);
                let mut supply = self.supply_by_class.get(&key).unwrap();
                supply -= 1;
                self.supply_by_class.insert(&key, &supply);
                tokens_burned_per_issuer += 1;
                tokens_burned += 1;
                if tokens_burned >= limit {
                    break;
                }
            }

            // update supply by owner
            let key = (owner.clone(), issuer_id);
            let mut supply = self.supply_by_owner.get(&key).unwrap();
            supply -= tokens_burned_per_issuer;
            self.supply_by_owner.insert(&key, &supply);

            // update total supply by issuer
            let mut supply = self.supply_by_issuer.get(&issuer_id).unwrap();
            supply -= tokens_burned_per_issuer;
            self.supply_by_issuer.insert(&issuer_id, &supply);

            SbtTokensEvent {
                issuer: issuer.to_owned(),
                tokens: token_ids.clone(),
            }
            .emit_burn();
            if tokens_burned >= limit {
                return false;
            }
        }
        true
    }

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

    fn assert_testnet(&self) {
        require!(
            env::current_account_id().as_str().contains("test"),
            "must be testnet"
        );
    }

    /// returns false if the `issuer` contract was already registered.
    pub fn testing_add_sbt_issuer(&mut self, issuer: AccountId) -> bool {
        self.assert_testnet();
        self._add_sbt_issuer(&issuer)
    }

    #[payable]
    pub fn testing_sbt_mint(
        &mut self,
        issuer: AccountId,
        token_spec: Vec<(AccountId, Vec<TokenMetadata>)>,
    ) -> Vec<TokenId> {
        self.assert_admins();
        self.assert_testnet();
        self._sbt_mint(&issuer, token_spec)
    }

    pub fn testing_sbt_renew(&mut self, issuer: AccountId, tokens: Vec<TokenId>, expires_at: u64) {
        self.assert_admins();
        self.assert_testnet();
        self._sbt_renew(issuer, tokens, expires_at)
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

    fn admins_flagged() -> Vec<AccountId> {
        vec![AccountId::new_unchecked("admin_flagged.near".to_string())]
    }

    #[inline]
    fn fractal_mainnet() -> AccountId {
        AccountId::new_unchecked("fractal.i-am-human.near".to_string())
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
        for i in 1..=n {
            batch_metadata.push(mk_metadata(i, Some(START + i)))
        }
        batch_metadata
    }

    fn max_gas() -> Gas {
        return Gas::ONE_TERA.mul(300);
    }

    const MILI_SECOND: u64 = 1_000_000; // milisecond in ns
    const START: u64 = 10;
    const MINT_DEPOSIT: Balance = 9 * MILI_NEAR;

    fn setup(predecessor: &AccountId, deposit: Balance) -> (VMContext, Contract) {
        let mut ctx = VMContextBuilder::new()
            .predecessor_account_id(admin())
            .block_timestamp(START * MILI_SECOND) // multiplying by mili seconds for easier testing
            .is_view(false)
            .build();
        if deposit > 0 {
            ctx.attached_deposit = deposit
        }
        testing_env!(ctx.clone());
        let mut ctr = Contract::new(admin(), fractal_mainnet(), vec![1], admins_flagged());
        ctr.admin_add_sbt_issuer(issuer1());
        ctr.admin_add_sbt_issuer(issuer2());
        ctr.admin_add_sbt_issuer(issuer3());
        ctr.admin_set_authorized_flaggers([predecessor.clone()].to_vec());
        ctx.predecessor_account_id = predecessor.clone();
        testing_env!(ctx.clone());
        return (ctx, ctr);
    }

    #[test]
    fn init_method() {
        let ctr = Contract::new(admin(), fractal_mainnet(), vec![1], vec![]);
        // make sure the iah_issuer has been set as an issuer
        assert_eq!(1, ctr.assert_issuer(&fractal_mainnet()));
    }

    #[test]
    fn iah_class_set() {
        let (_, ctr) = setup(&issuer1(), 2 * MINT_DEPOSIT);
        assert_eq!(ctr.iah_class_set(), vec![ctr.iah_sbts]);
    }

    #[test]
    fn add_sbt_issuer() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 2 * MINT_DEPOSIT);
        // in setup we add 4 issuers, so the next id will be 5.
        assert_eq!(5, ctr.next_issuer_id);
        assert_eq!(1, ctr.assert_issuer(&fractal_mainnet()));
        assert_eq!(2, ctr.assert_issuer(&issuer1()));
        assert_eq!(3, ctr.assert_issuer(&issuer2()));
        assert_eq!(4, ctr.assert_issuer(&issuer3()));

        assert_eq!(fractal_mainnet(), ctr.issuer_by_id(1));
        assert_eq!(issuer1(), ctr.issuer_by_id(2));
        assert_eq!(issuer2(), ctr.issuer_by_id(3));
        assert_eq!(issuer3(), ctr.issuer_by_id(4));

        ctx.predecessor_account_id = admin();
        testing_env!(ctx.clone());
        let ok = ctr.admin_add_sbt_issuer(issuer1());
        assert!(
            !ok,
            "isser1 should be already added, so it should return false"
        );
        assert_eq!(5, ctr.next_issuer_id, "next_issuer_id should not change");
        assert_eq!(
            2,
            ctr.assert_issuer(&issuer1()),
            "issuer1 id should not change"
        );
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
        let sbt1_1_e = mk_token(1, alice(), m1_1.clone());
        let sbt1_2_e = mk_token(2, bob(), m1_1.clone());
        assert_eq!(sbt1_1, sbt1_1_e);
        let sbt1_2 = ctr.sbt(issuer1(), 2).unwrap();
        assert_eq!(sbt1_2, sbt1_2_e);
        assert!(ctr.sbt(issuer2(), 1).is_none());
        assert!(ctr.sbt(issuer1(), 3).is_none());

        let sbts = ctr.sbts(issuer1(), vec![1, 2]);
        assert_eq!(sbts, vec![Some(sbt1_1_e.clone()), Some(sbt1_2_e.clone())]);
        assert_eq!(
            ctr.sbt_classes(issuer1(), vec![1, 1]),
            vec![Some(1), Some(1)]
        );

        let sbts = ctr.sbts(issuer1(), vec![2, 10, 3, 1]);
        assert_eq!(
            sbts,
            vec![Some(sbt1_2_e.clone()), None, None, Some(sbt1_1_e.clone())]
        );
        assert_eq!(
            ctr.sbt_classes(issuer1(), vec![2, 10, 3, 1]),
            vec![Some(1), None, None, Some(1)]
        );

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
            ctr.sbt_tokens(issuer1(), Some(1), None, None),
            vec![mk_token(1, alice2(), m1_1.clone())]
        );
        assert_eq!(ctr.sbt_tokens(issuer2(), None, None, None), t2_all,);
        assert_eq!(ctr.sbt_tokens(issuer2(), None, Some(1), None), t2_all[..1]);
        assert_eq!(ctr.sbt_tokens(issuer2(), None, Some(2), None), t2_all[..2]);
        assert_eq!(
            ctr.sbt_tokens(issuer2(), Some(2), Some(2), None),
            t2_all[1..3]
        );
        assert_eq!(
            ctr.sbt_tokens(issuer2(), Some(5), Some(5), None),
            t2_all[4..5]
        );
        assert_eq!(ctr.sbt_tokens(issuer2(), Some(6), Some(2), None), vec![]);

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
    #[should_panic(expected = "bob.near is not a human")]
    fn mint_iah() {
        let (mut ctx, mut ctr) = setup(&fractal_mainnet(), 150 * MINT_DEPOSIT);
        // issue IAH SBTs for alice
        let m1_1 = mk_metadata(1, Some(START)); // class=1 is IAH
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone()])]);

        ctx.predecessor_account_id = issuer1();
        testing_env!(ctx.clone());

        // alice is IAH verified, so mint_iah by issuer1 should work
        let sbts = ctr.sbt_mint_iah(vec![(alice(), vec![m1_1.clone()])]);
        assert!(!sbts.is_empty());

        // bob doesn't have IAH SBTs -> the mint below panics.
        ctr.sbt_mint_iah(vec![(bob(), vec![m1_1])]);
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
            ctr.sbt_tokens(issuer1(), None, None, None),
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

        let current_timestamp = ctx.block_timestamp / MILI_SECOND; // convert nano to mili seconds

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
            ctr.sbt_tokens(issuer1(), None, None, None),
            vec![
                mk_token(1, alice(), m1_1_revoked.clone()),
                mk_token(2, alice(), m2_1_revoked.clone()),
                mk_token(3, alice(), m3_1_revoked.clone())
            ]
        );
        assert_eq!(
            ctr.sbt_tokens(issuer2(), None, None, None),
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
        assert_eq!(test_utils::get_logs().len(), 2);
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
            ctr.sbt_tokens(issuer1(), None, None, None),
            vec![mk_token(4, alice(), m3_1.clone())],
        );
        assert_eq!(
            ctr.sbt_tokens(issuer2(), None, None, None),
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
    #[should_panic(expected = "account alice.nea is banned")]
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
        ctx.block_timestamp = START * MILI_SECOND; // 11 seconds
        testing_env!(ctx.clone());

        let m1_1 = mk_metadata(1, Some(START));
        let m1_2 = mk_metadata(2, Some(START));
        let m1_3 = mk_metadata(3, Some(START + 100));
        let m1_4 = mk_metadata(4, Some(START + 100));
        ctr.sbt_mint(vec![(alice(), vec![m1_1, m1_2, m1_3, m1_4])]);

        let res = ctr.sbt_tokens_by_owner(alice(), None, None, None, Some(true));
        assert_eq!(res[0].1.len(), 4);
        let res = ctr.sbt_tokens_by_owner(alice(), None, None, None, None);
        assert_eq!(res[0].1.len(), 4);

        let res = ctr.sbt_tokens(issuer1(), None, None, Some(true));
        assert_eq!(res.len(), 4);
        let res = ctr.sbt_tokens(issuer1(), None, None, Some(false));
        assert_eq!(res.len(), 4);
        let res = ctr.sbt_tokens(issuer1(), None, None, None);
        assert_eq!(res.len(), 4);

        // fast forward so the first two sbts are expired
        ctx.block_timestamp = (START + 50) * MILI_SECOND;
        testing_env!(ctx.clone());

        let res = ctr.sbt_tokens_by_owner(alice(), None, None, None, Some(true));
        assert_eq!(res[0].1.len(), 4);
        let res = ctr.sbt_tokens_by_owner(alice(), None, None, None, Some(false));
        assert_eq!(res[0].1.len(), 2);
        let res = ctr.sbt_tokens_by_owner(alice(), None, None, None, None);
        assert_eq!(res[0].1.len(), 2);

        let res = ctr.sbt_tokens(issuer1(), None, None, Some(true));
        assert_eq!(res.len(), 4);
        let res = ctr.sbt_tokens(issuer1(), None, None, Some(false));
        assert_eq!(res.len(), 2);
        let res = ctr.sbt_tokens(issuer1(), None, None, None);
        assert_eq!(res.len(), 2);
    }

    #[test]
    fn sbt_revoke_by_owner_basics() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 2 * MINT_DEPOSIT);

        let m1_1 = mk_metadata(1, Some(START + 100));
        let m1_2 = mk_metadata(2, Some(START + 100));
        let m1_1_expired = mk_metadata(1, Some(START));
        let m1_2_expired = mk_metadata(2, Some(START));

        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone(), m1_2.clone()])]);

        ctx.predecessor_account_id = issuer2();
        testing_env!(ctx.clone());

        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone(), m1_2.clone()])]);

        // revoke (burn) tokens minted for alice from issuer2
        ctr.sbt_revoke_by_owner(alice(), true);

        let log_burn = mk_log_str(
            "burn",
            &format!(r#"{{"issuer":"{}","tokens":[1,2]}}"#, issuer2()),
        );
        let log_revoke = mk_log_str(
            "revoke",
            &format!(r#"{{"issuer":"{}","tokens":[1,2]}}"#, issuer2()),
        );
        assert_eq!(test_utils::get_logs().len(), 3);
        assert_eq!(test_utils::get_logs()[1], log_burn[0]);
        assert_eq!(test_utils::get_logs()[2], log_revoke[0]);

        // make sure the balances are updated correctly
        let res = ctr.sbt_tokens_by_owner(alice(), None, None, None, None);
        assert!(res.len() == 1);
        assert_eq!(res[0].1.len(), 2);
        assert_eq!(ctr.sbt_supply(issuer1()), 2);
        assert_eq!(ctr.sbt_supply(issuer2()), 0);
        assert_eq!(ctr.sbt_supply_by_class(issuer1(), 1), 1);
        assert_eq!(ctr.sbt_supply_by_class(issuer1(), 2), 1);
        assert_eq!(ctr.sbt_supply_by_class(issuer2(), 1), 0);
        assert_eq!(ctr.sbt_supply_by_class(issuer2(), 2), 0);

        assert_eq!(
            ctr.sbt_tokens(issuer1(), None, None, None),
            vec![
                mk_token(1, alice(), m1_1.clone()),
                mk_token(2, alice(), m1_2.clone()),
            ],
        );
        assert!(ctr.sbt_tokens(issuer2(), None, None, None).len() == 0);

        // revoke (not burn) tokens minted for alice from issuer1
        ctx.predecessor_account_id = issuer1();
        testing_env!(ctx.clone());
        assert_eq!(test_utils::get_logs().len(), 0);
        ctr.sbt_revoke_by_owner(alice(), false);

        let log_revoke = mk_log_str(
            "revoke",
            &format!(r#"{{"issuer":"{}","tokens":[1,2]}}"#, issuer1()),
        );
        assert_eq!(test_utils::get_logs().len(), 1);
        assert_eq!(test_utils::get_logs()[0], log_revoke[0]);

        // fast forward
        ctx.block_timestamp = (START + 50) * MILI_SECOND;
        testing_env!(ctx.clone());

        // make sure the balances are updated correctly
        let res_with_expired = ctr.sbt_tokens_by_owner(alice(), None, None, None, None);
        assert!(res_with_expired.len() == 0);
        let res_without_expired = ctr.sbt_tokens_by_owner(alice(), None, None, None, Some(true));
        assert!(res_without_expired.len() == 1);
        assert_eq!(res[0].1.len(), 2);
        assert_eq!(ctr.sbt_supply(issuer1()), 2);
        assert_eq!(ctr.sbt_supply(issuer2()), 0);
        assert_eq!(ctr.sbt_supply_by_class(issuer1(), 1), 1);
        assert_eq!(ctr.sbt_supply_by_class(issuer1(), 2), 1);
        assert_eq!(ctr.sbt_supply_by_class(issuer2(), 1), 0);
        assert_eq!(ctr.sbt_supply_by_class(issuer2(), 2), 0);

        assert_eq!(
            ctr.sbt_tokens(issuer1(), None, None, Some(true)),
            vec![
                mk_token(1, alice(), m1_1_expired.clone()),
                mk_token(2, alice(), m1_2_expired.clone()),
            ],
        );
        assert!(ctr.sbt_tokens(issuer1(), None, None, None).len() == 0);
        assert!(ctr.sbt_tokens(issuer2(), None, None, None).len() == 0);
    }

    #[test]
    fn sbt_revoke_by_owner_batch() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 20 * MINT_DEPOSIT);

        // mint tokens to alice and bob from issuer1
        let batch_metadata = mk_batch_metadata(20);
        ctr.sbt_mint(vec![(alice(), batch_metadata[..10].to_vec())]);
        ctr.sbt_mint(vec![(bob(), batch_metadata[10..].to_vec())]);

        // mint tokens to alice and bob from issuer2
        ctx.predecessor_account_id = issuer2();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), batch_metadata[..10].to_vec())]);
        ctr.sbt_mint(vec![(bob(), batch_metadata[11..].to_vec())]);

        let res = ctr.sbt_tokens_by_owner(alice(), None, None, None, None);
        assert_eq!(res[0].1.len(), 10);
        assert_eq!(res[1].1.len(), 10);

        let res = ctr.sbt_tokens_by_owner(bob(), None, None, None, None);
        assert_eq!(res[0].1.len(), 10);
        assert_eq!(res[1].1.len(), 9);

        assert_eq!(ctr.sbt_supply(issuer1()), 20);
        assert_eq!(ctr.sbt_supply(issuer2()), 19);

        // revoke (burn) tokens minted for alice from issuer2
        ctr.sbt_revoke_by_owner(alice(), true);

        // make sure the balances are updated correctly
        let res = ctr.sbt_tokens_by_owner(alice(), None, None, None, None);
        assert_eq!(res[0].1.len(), 10);
        // assert_eq!(res[1].1.len(), 0);

        let res = ctr.sbt_tokens_by_owner(bob(), None, None, None, None);
        assert_eq!(res[0].1.len(), 10);
        assert_eq!(res[1].1.len(), 9);

        assert_eq!(ctr.sbt_supply(issuer1()), 20);
        assert_eq!(ctr.sbt_supply(issuer2()), 9);
    }

    #[test]
    fn is_human() {
        let (mut ctx, mut ctr) = setup(&fractal_mainnet(), 150 * MINT_DEPOSIT);
        ctx.current_account_id = AccountId::new_unchecked("registry.i-am-human.near".to_string());
        testing_env!(ctx.clone());

        let m1_1 = mk_metadata(1, Some(START));
        let m1_2 = mk_metadata(2, Some(START));
        ctr.sbt_mint(vec![(alice(), vec![m1_1])]);
        ctr.sbt_mint(vec![(bob(), vec![m1_2])]);

        assert_eq!(ctr.is_human(alice()), vec![(fractal_mainnet(), vec![1])]);
        assert_eq!(ctr.is_human(bob()), vec![]);

        // step forward, so the tokens will expire
        ctx.block_timestamp = (START + 1) * MILI_SECOND;
        testing_env!(ctx.clone());
        assert_eq!(ctr.is_human(alice()), vec![]);
        assert_eq!(ctr.is_human(bob()), vec![]);
    }

    #[test]
    fn is_human_expires_at_none() {
        let (_, mut ctr) = setup(&fractal_mainnet(), 150 * MINT_DEPOSIT);

        // make sure is_human works as expected when the expiratoin date is set to None (the token never expires).
        let m1_1 = mk_metadata(1, None);
        ctr.sbt_mint(vec![(alice(), vec![m1_1])]);

        assert_eq!(ctr.is_human(alice()), vec![(fractal_mainnet(), vec![1])]);
    }

    #[test]
    fn is_human_multiple_classes() {
        let (mut ctx, mut ctr) = setup(&fractal_mainnet(), 150 * MINT_DEPOSIT);
        ctr.iah_sbts.1 = vec![1, 3];
        ctx.current_account_id = AccountId::new_unchecked("registry.i-am-human.near".to_string());
        testing_env!(ctx.clone());

        let m1_1 = mk_metadata(1, Some(START));
        let m1_2 = mk_metadata(2, Some(START));
        let m1_3 = mk_metadata(3, Some(START));
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone()])]);
        ctr.sbt_mint(vec![(bob(), vec![m1_2.clone()])]);
        ctr.sbt_mint(vec![(carol(), vec![m1_2, m1_1.clone()])]);
        ctr.sbt_mint(vec![(dan(), vec![m1_3, m1_1])]);

        assert_eq!(ctr.is_human(alice()), vec![]);
        assert_eq!(ctr.is_human(bob()), vec![]);
        assert_eq!(ctr.is_human(carol()), vec![]);
        assert_eq!(ctr.is_human(dan()), vec![(fractal_mainnet(), vec![6, 5])]);
    }

    #[test]
    fn sbt_tokens_by_owner_per_issuer() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 20 * MINT_DEPOSIT);
        let batch_metadata = mk_batch_metadata(30);
        ctr.sbt_mint(vec![(alice(), batch_metadata[..10].to_vec())]);

        ctx.predecessor_account_id = issuer3();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), batch_metadata[10..20].to_vec())]);

        ctx.predecessor_account_id = issuer2();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), batch_metadata[20..].to_vec())]);

        let res = ctr.sbt_tokens_by_owner(alice(), None, None, None, None);
        assert_eq!(res.len(), 3);
        assert_eq!(res[0].1.len(), 10);
        assert_eq!(res[1].1.len(), 10);
        assert_eq!(res[2].1.len(), 10);
        // assert that returns always in the ascending order (not minting order)
        assert_eq!(res[0].0, issuer1());
        assert_eq!(res[1].0, issuer2());
        assert_eq!(res[2].0, issuer3());

        let expected_tokens: Vec<u64> = (1..=10).collect();

        let res = ctr.sbt_tokens_by_owner(alice(), Some(issuer1()), None, None, None);
        assert_eq!(res.len(), 1);
        assert_eq!(
            res[0].1.iter().map(|t| t.token).collect::<Vec<u64>>(),
            expected_tokens,
        );
        let res = ctr.sbt_tokens_by_owner(alice(), Some(issuer2()), None, None, None);
        assert_eq!(res.len(), 1);
        assert_eq!(
            res[0].1.iter().map(|t| t.token).collect::<Vec<u64>>(),
            expected_tokens,
        );
        let res = ctr.sbt_tokens_by_owner(alice(), Some(issuer3()), None, None, None);
        assert_eq!(res.len(), 1);
        assert_eq!(
            res[0].1.iter().map(|t| t.token).collect::<Vec<u64>>(),
            expected_tokens,
        );

        // mint more tokens for issuer1()
        ctx.predecessor_account_id = issuer1();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), batch_metadata[20..30].to_vec())]);
        let res = ctr.sbt_tokens_by_owner(alice(), Some(issuer1()), None, None, None);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].0, issuer1());
        assert_eq!(
            res[0].1.iter().map(|t| t.token).collect::<Vec<u64>>(),
            (1..=20).collect::<Vec<u64>>()
        );
    }

    #[test]
    fn is_human_multiple_classes_with_expired_tokens() {
        let (mut ctx, mut ctr) = setup(&fractal_mainnet(), 150 * MINT_DEPOSIT);
        ctr.iah_sbts.1 = vec![1, 3];
        ctx.current_account_id = AccountId::new_unchecked("registry.i-am-human.near".to_string());
        testing_env!(ctx.clone());

        let m1_1 = mk_metadata(1, Some(START + 100));
        let m1_2 = mk_metadata(2, Some(START + 100));
        let m1_3 = mk_metadata(3, Some(START));
        ctr.sbt_mint(vec![(alice(), vec![m1_1, m1_2, m1_3])]);

        assert_eq!(ctr.is_human(alice()), vec![(fractal_mainnet(), vec![1, 3])]);
        // step forward, so token class==3 will expire
        ctx.block_timestamp = (START + 1) * MILI_SECOND;
        testing_env!(ctx.clone());
        assert_eq!(ctr.is_human(alice()), vec![]);
    }

    #[test]
    fn sbt_revoke_events() {
        let (ctx, mut ctr) = setup(&fractal_mainnet(), 2 * MINT_DEPOSIT);
        let m1_1 = mk_metadata(1, Some(START));
        let tokens = ctr.sbt_mint(vec![(alice(), vec![m1_1])]);

        // clear the events
        testing_env!(ctx.clone());

        // revoke (burn == false)
        ctr.sbt_revoke(vec![tokens[0]], false);

        let log_revoke = mk_log_str(
            "revoke",
            &format!(r#"{{"issuer":"{}","tokens":[1]}}"#, fractal_mainnet()),
        );
        let log_burn = mk_log_str(
            "burn",
            &format!(r#"{{"issuer":"{}","tokens":[1]}}"#, fractal_mainnet()),
        );

        // check only revoke event is emitted
        assert_eq!(test_utils::get_logs().len(), 1);
        assert_eq!(test_utils::get_logs(), log_revoke);

        // clear the events
        testing_env!(ctx.clone());

        // revoke (burn == true)
        ctr.sbt_revoke(tokens, true);

        // check both burn and revoke events are emitted
        assert_eq!(test_utils::get_logs().len(), 2); // -> only 1 event is emmited
        assert_eq!(test_utils::get_logs(), vec![log_burn, log_revoke].concat());
        // -> missing revoke event
    }

    #[test]
    fn sbt_burn_all_more_users() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 20 * MINT_DEPOSIT);

        // mint tokens to alice and bob from issuer1
        let batch_metadata = mk_batch_metadata(20);
        ctr.sbt_mint(vec![(alice(), batch_metadata[..10].to_vec())]);
        ctr.sbt_mint(vec![(bob(), batch_metadata[10..].to_vec())]);

        // mint tokens to alice and bob from issuer2
        ctx.predecessor_account_id = issuer2();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), batch_metadata[..10].to_vec())]);
        ctr.sbt_mint(vec![(bob(), batch_metadata[11..].to_vec())]);

        // mint tokens to alice and bob from issuer3
        ctx.predecessor_account_id = issuer3();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), batch_metadata[..10].to_vec())]);
        ctr.sbt_mint(vec![(bob(), batch_metadata[10..].to_vec())]);

        let res = ctr.sbt_tokens_by_owner(alice(), None, None, None, None);
        assert_eq!(res[0].1.len(), 10);
        assert_eq!(res[1].1.len(), 10);
        assert_eq!(res[2].1.len(), 10);

        let res = ctr.sbt_tokens_by_owner(bob(), None, None, None, None);
        assert_eq!(res[0].1.len(), 10);
        assert_eq!(res[1].1.len(), 9);
        assert_eq!(res[2].1.len(), 10);

        assert_eq!(ctr.sbt_supply(issuer1()), 20);
        assert_eq!(ctr.sbt_supply(issuer2()), 19);
        assert_eq!(ctr.sbt_supply(issuer3()), 20);

        // alice burn all her tokens from all the issuers
        ctx.predecessor_account_id = alice();
        testing_env!(ctx.clone());
        let res = ctr._sbt_burn_all(20);
        assert!(!res);
        let res = ctr._sbt_burn_all(20);
        assert!(res); // make sure that after the second call true is returned (all tokens have been burned)

        // make sure the balances are updated correctly
        let res = ctr.sbt_tokens_by_owner(alice(), None, None, None, None);
        assert!(res.is_empty());

        let res = ctr.sbt_tokens_by_owner(bob(), None, None, None, None);
        assert_eq!(res[0].1.len(), 10);
        assert_eq!(res[1].1.len(), 9);
        assert_eq!(res[2].1.len(), 10);

        assert_eq!(ctr.sbt_supply(issuer1()), 10);
        assert_eq!(ctr.sbt_supply(issuer2()), 9);
        assert_eq!(ctr.sbt_supply(issuer3()), 10);
    }

    #[test]
    fn sbt_burn_all_basics() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 20 * MINT_DEPOSIT);

        // mint tokens to alice and bob from issuer1
        let batch_metadata = mk_batch_metadata(20);
        ctr.sbt_mint(vec![(alice(), batch_metadata[..10].to_vec())]);

        // mint tokens to alice and bob from issuer2
        ctx.predecessor_account_id = issuer2();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), batch_metadata[..10].to_vec())]);

        // mint tokens to alice and bob from issuer3
        ctx.predecessor_account_id = issuer3();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), batch_metadata[..10].to_vec())]);

        // alice burn all her tokens from all the issuers
        ctx.predecessor_account_id = alice();
        testing_env!(ctx.clone());
        loop {
            if ctr._sbt_burn_all(10) {
                break;
            }
        }

        // check if the logs are correct
        assert_eq!(test_utils::get_logs().len(), 3);

        let log_burn_issuer_1 = mk_log_str(
            "burn",
            &format!(
                r#"{{"issuer":"{}","tokens":[1,2,3,4,5,6,7,8,9,10]}}"#,
                issuer1()
            ),
        );

        let log_burn_issuer_2 = mk_log_str(
            "burn",
            &format!(
                r#"{{"issuer":"{}","tokens":[1,2,3,4,5,6,7,8,9,10]}}"#,
                issuer2()
            ),
        );

        let log_burn_issuer_3 = mk_log_str(
            "burn",
            &format!(
                r#"{{"issuer":"{}","tokens":[1,2,3,4,5,6,7,8,9,10]}}"#,
                issuer3()
            ),
        );

        assert_eq!(test_utils::get_logs()[0], log_burn_issuer_1[0]);
        assert_eq!(test_utils::get_logs()[1], log_burn_issuer_2[0]);
        assert_eq!(test_utils::get_logs()[2], log_burn_issuer_3[0]);

        // make sure the balances are updated correctly
        let res = ctr.sbt_tokens_by_owner(alice(), None, None, None, None);
        assert!(res.is_empty());

        assert_eq!(ctr.sbt_supply(issuer1()), 0);
        assert_eq!(ctr.sbt_supply(issuer2()), 0);
        assert_eq!(ctr.sbt_supply(issuer3()), 0);
        for i in 1..=10 {
            print!("{}", i);
            assert_eq!(ctr.sbt_supply_by_class(issuer1(), i), 0);
        }
        for i in 1..=10 {
            assert_eq!(ctr.sbt_supply_by_class(issuer2(), i), 0);
        }
        for i in 1..=10 {
            assert_eq!(ctr.sbt_supply_by_class(issuer3(), i), 0);
        }
    }
    #[test]
    fn sbt_burn_all_limit() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 60 * MINT_DEPOSIT);

        // mint tokens to alice and bob from issuer1
        let batch_metadata = mk_batch_metadata(40);
        ctr.sbt_mint(vec![(alice(), batch_metadata[..20].to_vec())]);
        ctr.sbt_mint(vec![(bob(), batch_metadata[20..].to_vec())]);

        // mint tokens to alice and bob from issuer2
        ctx.predecessor_account_id = issuer2();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), batch_metadata[..20].to_vec())]);
        ctr.sbt_mint(vec![(bob(), batch_metadata[20..].to_vec())]);

        // mint tokens to alice and bob from issuer3
        ctx.predecessor_account_id = issuer3();
        testing_env!(ctx.clone());
        ctr.sbt_mint(vec![(alice(), batch_metadata[..20].to_vec())]);
        ctr.sbt_mint(vec![(bob(), batch_metadata[20..].to_vec())]);

        assert_eq!(ctr.sbt_supply(issuer1()), 40);
        assert_eq!(ctr.sbt_supply(issuer2()), 40);
        assert_eq!(ctr.sbt_supply(issuer3()), 40);

        // alice burn all her tokens from all the issuers
        ctx.predecessor_account_id = alice();
        loop {
            ctx.prepaid_gas = max_gas();
            testing_env!(ctx.clone());
            if ctr._sbt_burn_all(41) {
                //anything above 41 fails due to MaxGasLimitExceeded error
                break;
            }
        }

        // make sure the balances are updated correctly
        let res = ctr.sbt_tokens_by_owner(alice(), None, None, None, None);
        assert!(res.is_empty());

        let res = ctr.sbt_tokens_by_owner(bob(), None, None, None, None);
        assert_eq!(res[0].1.len(), 20);
        assert_eq!(res[1].1.len(), 20);
        assert_eq!(res[2].1.len(), 20);

        assert_eq!(ctr.sbt_supply(issuer1()), 20);
        assert_eq!(ctr.sbt_supply(issuer2()), 20);
        assert_eq!(ctr.sbt_supply(issuer3()), 20);
    }

    #[test]
    fn is_human_call() {
        let (mut ctx, mut ctr) = setup(&fractal_mainnet(), MINT_DEPOSIT);

        let m1_1 = mk_metadata(1, Some(START));
        ctr.sbt_mint(vec![(alice(), vec![m1_1])]);
        assert_eq!(ctr.is_human(alice()), vec![(fractal_mainnet(), vec![1])]);

        ctx.predecessor_account_id = alice();
        testing_env!(ctx.clone());

        ctr.is_human_call(
            AccountId::new_unchecked("registry.i-am-human.near".to_string()),
            "function_name".to_string(),
            "{}".to_string(),
        );
    }

    #[test]
    #[should_panic(expected = "caller not a human")]
    fn is_human_call_fail() {
        let (_, mut ctr) = setup(&alice(), MINT_DEPOSIT);

        ctr.is_human_call(
            AccountId::new_unchecked("registry.i-am-human.near".to_string()),
            "function_name".to_string(),
            "{}".to_string(),
        );
    }

    #[test]
    fn admin_set_authorized_flaggers() {
        let (mut ctx, mut ctr) = setup(&admin(), MINT_DEPOSIT);

        let flaggers = [dan()].to_vec();
        ctr.admin_set_authorized_flaggers(flaggers);

        ctx.predecessor_account_id = dan();
        testing_env!(ctx);
        ctr.assert_authorized_flagger();
    }

    #[test]
    #[should_panic(expected = "not an admin")]
    fn admin_set_authorized_flaggers_fail() {
        let (mut ctx, mut ctr) = setup(&admin(), MINT_DEPOSIT);

        ctx.predecessor_account_id = dan();
        testing_env!(ctx.clone());

        let flaggers = [dan()].to_vec();
        ctr.admin_set_authorized_flaggers(flaggers);
    }

    #[test]
    fn admin_flag_accounts() {
        let (_, mut ctr) = setup(&alice(), MINT_DEPOSIT);

        ctr.admin_flag_accounts(
            AccountFlag::Blacklisted,
            [dan(), issuer1()].to_vec(),
            "memo".to_owned(),
        );
        ctr.admin_flag_accounts(
            AccountFlag::Verified,
            [issuer2()].to_vec(),
            "memo".to_owned(),
        );

        let exp = r#"EVENT_JSON:{"standard":"i_am_human","version":"1.0.0","event":"flag_blacklisted","data":["dan.near","sbt.n"]}"#;
        // check only flag event is emitted
        assert_eq!(test_utils::get_logs().len(), 2);
        assert_eq!(test_utils::get_logs()[0], exp);

        assert_eq!(ctr.account_flagged(dan()), Some(AccountFlag::Blacklisted));
        assert_eq!(
            ctr.account_flagged(issuer1()),
            Some(AccountFlag::Blacklisted)
        );
        assert_eq!(ctr.account_flagged(issuer2()), Some(AccountFlag::Verified));

        ctr.admin_unflag_accounts([dan()].to_vec(), "memo".to_owned());

        let exp = r#"EVENT_JSON:{"standard":"i_am_human","version":"1.0.0","event":"unflag","data":["dan.near"]}"#;
        assert_eq!(test_utils::get_logs().len(), 3);
        assert_eq!(test_utils::get_logs()[2], exp);

        assert_eq!(ctr.account_flagged(dan()), None);
        assert_eq!(
            ctr.account_flagged(issuer1()),
            Some(AccountFlag::Blacklisted)
        );
    }

    #[test]
    #[should_panic(expected = "not authorized")]
    fn admin_flag_accounts_non_authorized() {
        let (mut ctx, mut ctr) = setup(&alice(), MINT_DEPOSIT);

        ctx.predecessor_account_id = dan();
        testing_env!(ctx.clone());
        ctr.admin_flag_accounts(AccountFlag::Blacklisted, vec![dan()], "memo".to_owned());
    }

    #[test]
    #[should_panic(expected = "account bob.near is banned")]
    fn admin_flag_accounts_banned() {
        let (_, mut ctr) = setup(&alice(), MINT_DEPOSIT);

        ctr.banlist.insert(&bob());
        ctr.admin_flag_accounts(
            AccountFlag::Blacklisted,
            vec![dan(), bob()],
            "memo".to_owned(),
        );
    }

    #[test]
    #[should_panic(expected = "not authorized")]
    fn admin_unflag_accounts_non_authorized() {
        let (mut ctx, mut ctr) = setup(&alice(), MINT_DEPOSIT);

        ctr.admin_flag_accounts(
            AccountFlag::Blacklisted,
            vec![dan(), issuer1()],
            "memo".to_owned(),
        );
        assert_eq!(ctr.account_flagged(dan()), Some(AccountFlag::Blacklisted));

        ctx.predecessor_account_id = dan();
        testing_env!(ctx.clone());
        ctr.admin_unflag_accounts(vec![dan()], "memo".to_owned());
    }

    #[test]
    fn is_human_flagged() {
        let (_, mut ctr) = setup(&fractal_mainnet(), MINT_DEPOSIT);

        let m1_1 = mk_metadata(1, Some(START));
        ctr.sbt_mint(vec![(dan(), vec![m1_1])]);
        let human_proof = vec![(fractal_mainnet(), vec![1])];
        ctr.admin_flag_accounts(AccountFlag::Verified, [dan()].to_vec(), "memo".to_owned());
        assert_eq!(ctr.is_human(dan()), human_proof.clone());

        ctr.admin_flag_accounts(
            AccountFlag::Blacklisted,
            [dan()].to_vec(),
            "memo".to_owned(),
        );
        assert_eq!(ctr.is_human(dan()), vec![]);

        ctr.admin_unflag_accounts([dan()].to_vec(), "memo".to_owned());
        assert_eq!(ctr.is_human(dan()), human_proof);
    }

    #[test]
    #[should_panic(
        expected = "can't transfer soul from a blacklisted account to a verified account"
    )]
    fn flagged_soul_transfer() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 2 * MINT_DEPOSIT);

        let m1_1 = mk_metadata(1, Some(START + 10));
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone()])]);
        ctr.admin_flag_accounts(AccountFlag::Blacklisted, vec![alice()], "memo".to_owned());
        ctr.admin_flag_accounts(AccountFlag::Verified, vec![bob()], "memo".to_owned());

        // make soul transfer
        ctx.predecessor_account_id = alice();
        testing_env!(ctx.clone());
        ctr.sbt_soul_transfer(alice2(), None);

        assert_eq!(
            ctr.flagged.get(&alice()),
            Some(AccountFlag::Blacklisted),
            "flag must not be removed"
        );
        assert_eq!(
            ctr.flagged.get(&alice2()),
            Some(AccountFlag::Blacklisted),
            "flag is transferred"
        );
        assert_eq!(
            ctr.flagged.get(&bob()),
            Some(AccountFlag::Verified),
            "bob keeps his flag"
        );

        // transferring from blacklisted to verified account should fail
        ctx.predecessor_account_id = alice2();
        testing_env!(ctx.clone());
        ctr.sbt_soul_transfer(bob(), None);
    }

    #[test]
    #[should_panic(
        expected = "can't transfer soul from a verified account to a blacklisted account"
    )]
    fn flagged_soul_transfer2() {
        let (mut ctx, mut ctr) = setup(&issuer1(), 2 * MINT_DEPOSIT);

        let m1_1 = mk_metadata(1, Some(START + 10));
        ctr.sbt_mint(vec![(alice(), vec![m1_1.clone()])]);
        ctr.admin_flag_accounts(AccountFlag::Verified, vec![alice()], "memo".to_owned());
        ctr.admin_flag_accounts(AccountFlag::Blacklisted, vec![alice2()], "memo".to_owned());

        ctx.predecessor_account_id = alice();
        testing_env!(ctx.clone());
        ctr.sbt_soul_transfer(alice2(), None);
    }
}
