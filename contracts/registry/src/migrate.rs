use crate::*;

#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct OldState {
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

#[near_bindgen]
impl Contract {
    #[private]
    #[init(ignore_state)]
    pub fn migrate(iah_issuer: AccountId, iah_classes: Vec<ClassId>) -> Self {
        // retrieve the current state from the contract
        let old_state: OldState = env::state_read().expect("failed");
        // new field in the smart contract : pub(crate) iah_classes: (AccountId, Vec<ClassId>),

        Self {
            authority: old_state.authority.clone(),
            sbt_issuers: old_state.sbt_issuers,
            issuer_id_map: old_state.issuer_id_map,
            banlist: old_state.banlist,
            ongoing_soul_tx: old_state.ongoing_soul_tx,
            supply_by_owner: old_state.supply_by_owner,
            supply_by_class: old_state.supply_by_class,
            supply_by_issuer: old_state.supply_by_issuer,
            balances: old_state.balances,
            issuer_tokens: old_state.issuer_tokens,
            next_token_ids: old_state.next_token_ids,
            next_issuer_id: old_state.next_issuer_id,
            iah_classes: (iah_issuer.clone(), iah_classes.clone()),
        }
    }
}
