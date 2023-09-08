use crate::*;

// registry/v1.3.0
#[derive(BorshDeserialize, PanicOnDefault)]
pub struct OldState {
    pub authority: AccountId,
    pub sbt_issuers: UnorderedMap<AccountId, IssuerId>,
    pub issuer_id_map: LookupMap<IssuerId, AccountId>, // reverse index
    pub(crate) banlist: UnorderedSet<AccountId>,
    pub(crate) ongoing_soul_tx: LookupMap<AccountId, IssuerTokenId>,
    pub(crate) supply_by_owner: LookupMap<(AccountId, IssuerId), u64>,
    pub(crate) supply_by_class: LookupMap<(IssuerId, ClassId), u64>,
    pub(crate) supply_by_issuer: LookupMap<IssuerId, u64>,
    pub(crate) balances: TreeMap<BalanceKey, TokenId>,
    pub(crate) issuer_tokens: LookupMap<IssuerTokenId, TokenData>,
    pub(crate) next_token_ids: LookupMap<IssuerId, TokenId>,
    pub(crate) next_issuer_id: IssuerId,
    pub(crate) iah_sbts: (AccountId, Vec<ClassId>),
}

#[near_bindgen]
impl Contract {
    #[private]
    #[init(ignore_state)]
    #[allow(dead_code)] // no migration for 1.5.0
                        /* pub  */
    fn migrate(authorized_flaggers: Vec<AccountId>) -> Self {
        let old_state: OldState = env::state_read().expect("failed");
        // new field in the smart contract :
        // + flagged: LookupMap<AccountId, AccountFlag>
        // + authorized_flaggers: LazyOption<Vec<AccountId>>

        Self {
            authority: old_state.authority.clone(),
            sbt_issuers: old_state.sbt_issuers,
            issuer_id_map: old_state.issuer_id_map,
            banlist: old_state.banlist,
            supply_by_owner: old_state.supply_by_owner,
            supply_by_class: old_state.supply_by_class,
            supply_by_issuer: old_state.supply_by_issuer,
            balances: old_state.balances,
            issuer_tokens: old_state.issuer_tokens,
            next_token_ids: old_state.next_token_ids,
            next_issuer_id: old_state.next_issuer_id,
            ongoing_soul_tx: old_state.ongoing_soul_tx,
            iah_sbts: old_state.iah_sbts,
            flagged: LookupMap::new(StorageKey::Flagged),
            authorized_flaggers: LazyOption::new(
                StorageKey::AdminsFlagged,
                Some(&authorized_flaggers),
            ),
        }
    }
}
