use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, TreeMap, UnorderedMap, UnorderedSet};
use near_sdk::{env, near_bindgen, require, AccountId, PanicOnDefault};

use sbt::{emit_soul_transfer, ClassId, TokenData, TokenId};

use crate::storage::*;

mod registry;
mod storage;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// Registry admin, expected to be a DAO.
    pub authority: AccountId,

    /// registry of approved SBT contracts to issue tokens
    pub sbt_contracts: UnorderedMap<AccountId, CtrId>,
    pub ctr_id_map: LookupMap<CtrId, AccountId>, // reverse index
    /// registry of blacklisted accounts by issuer
    pub banlist: UnorderedSet<AccountId>,

    pub(crate) supply_by_owner: LookupMap<(AccountId, CtrId), u64>,
    pub(crate) supply_by_class: LookupMap<(CtrId, ClassId), u64>,
    pub(crate) supply_by_ctr: LookupMap<CtrId, u64>,
    /// maps user account to list of token source info
    pub(crate) balances: TreeMap<BalanceKey, TokenId>,
    /// maps SBT contract -> map of tokens
    pub(crate) ctr_tokens: LookupMap<CtrTokenId, TokenData>,
    /// map of SBT contract -> next available token_id
    pub(crate) next_token_ids: LookupMap<CtrId, TokenId>,
    pub(crate) next_ctr_id: CtrId,
    pub(crate) ongoing_soul_tx: LookupMap<AccountId, CtrTokenId>,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(authority: AccountId) -> Self {
        Self {
            authority,
            sbt_contracts: UnorderedMap::new(StorageKey::SbtContracts),
            ctr_id_map: LookupMap::new(StorageKey::SbtContractsRev),
            banlist: UnorderedSet::new(StorageKey::Banlist),
            supply_by_owner: LookupMap::new(StorageKey::SupplyByOwner),
            supply_by_class: LookupMap::new(StorageKey::SupplyByClass),
            supply_by_ctr: LookupMap::new(StorageKey::SupplyByCtr),
            balances: TreeMap::new(StorageKey::Balances),
            ctr_tokens: LookupMap::new(StorageKey::CtrTokens),
            next_token_ids: LookupMap::new(StorageKey::NextTokenId),
            next_ctr_id: 1,
            ongoing_soul_tx: LookupMap::new(StorageKey::OngoingSoultTx),
        }
    }

    //
    // Queries
    //

    pub fn sbt_contracts(&self) -> Vec<AccountId> {
        self.sbt_contracts.keys().collect()
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
    /// Returns the lastly moved SBT identified by it's contract issuer and token ID as well
    /// a boolean: `true` if the whole process has finished, `false` when the process has not
    /// finished and should be continued by a subsequent call.
    /// User must keeps calling `sbt_soul_transfer` until `true` is returned.
    /// Must emit `SoulTransfer` event.
    #[payable]
    pub fn sbt_soul_transfer(
        &mut self,
        to: AccountId,
        memo: Option<String>,
    ) -> (AccountId, TokenId, bool) {
        let owner = env::predecessor_account_id();
        let start = match self.ongoing_soul_tx.get(&to) {
            // starting the process
            None => {
                // insert into banlist and assuer owner is not already banned.
                require!(
                    self.banlist.insert(&owner),
                    "caller banned: can't make soul transfer"
                );
                require!(!self._is_banned(&to), "`to` is banned");
                emit_soul_transfer(&owner, &to, memo);
                CtrTokenId {
                    ctr_id: 0,
                    token: 0,
                }
            }
            // resuming Soul Transfer process
            Some(s) => s,
        };

        println!("Starting at: {} {}", start.ctr_id, start.token);
        env::panic_str("not implemented");
    }

    // TODO
    // pub fn sbt_burn(&mut self, ctr: AccountId, token: TokenId, memo: Option<String>) {
    //     emit Burn
    //     env::panic_str("not implemented");
    // }

    //
    // Authority
    //

    /// returns false if the `issuer` contract was already registered.
    pub fn admin_add_sbt_issuer(&mut self, issuer: AccountId) -> bool {
        self.assert_authority();
        let previous = self.sbt_contracts.insert(&issuer, &self.next_ctr_id);
        self.ctr_id_map.insert(&self.next_ctr_id, &issuer);
        self.next_ctr_id += 1;
        previous.is_none()
    }

    //
    // Internal
    //

    pub(crate) fn ctr_id(&self, ctr: &AccountId) -> CtrId {
        // TODO: use Result rather than panic
        self.sbt_contracts.get(ctr).expect("SBT Issuer not found")
    }

    // pub(crate) fn get_user_balances(&self, user: &AccountId) -> UnorderedMap<CtrClassId, TokenId> {
    //     self.balances
    //         .get(user)
    //         // TODO: verify how this works
    //         .unwrap_or_else(|| {
    //             UnorderedMap::new(StorageKey::BalancesMap {
    //                 owner: user.clone(),
    //             })
    //         })
    // }

    /// updates the internal token counter based on how many tokens we want to mint (num), and
    /// returns the first valid TokenId for newly minted tokens.
    pub(crate) fn next_token_id(&mut self, ctr_id: CtrId, num: u64) -> TokenId {
        let tid = self.next_token_ids.get(&ctr_id).unwrap_or(0);
        self.next_token_ids.insert(&ctr_id, &(tid + num));
        tid + 1
    }

    #[inline]
    pub(crate) fn assert_not_banned(&self, owner: &AccountId) {
        require!(
            !self.banlist.contains(owner),
            format!("account {} is banned", owner)
        );
    }

    /// note: use ctr_id() if you need ctr_id
    pub(crate) fn assert_issuer(&self, contract: &AccountId) {
        require!(self.sbt_contracts.get(contract).is_some())
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
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::{testing_env, VMContext};
    use sbt::*;

    use pretty_assertions::{assert_eq, assert_ne};

    use super::*;

    // TODO
    #[allow(dead_code)]

    fn alice() -> AccountId {
        AccountId::new_unchecked("alice.near".to_string())
    }

    fn a_user() -> AccountId {
        AccountId::new_unchecked("alice.nea".to_string())
    }

    fn bob() -> AccountId {
        AccountId::new_unchecked("bob.near".to_string())
    }

    fn issuer1() -> AccountId {
        AccountId::new_unchecked("sbt.ne".to_string())
    }

    fn issuer2() -> AccountId {
        AccountId::new_unchecked("sbt.nea".to_string())
    }

    fn issuer3() -> AccountId {
        AccountId::new_unchecked("sbt.near".to_string())
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

    const START: u64 = 10;

    fn setup(predecessor: &AccountId) -> (VMContext, Contract) {
        let mut ctx = VMContextBuilder::new()
            .predecessor_account_id(admin())
            // .attached_deposit(deposit_dec.into())
            .block_timestamp(START)
            .is_view(false)
            .build();
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
        let (_, mut ctr) = setup(&issuer1());
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
        let (mut ctx, mut ctr) = setup(&issuer1());
        let m1_1 = mk_metadata(1, Some(START + 10));
        let m1_2 = mk_metadata(1, Some(START + 12));
        let m2_1 = mk_metadata(2, Some(START + 14));
        let m4_1 = mk_metadata(4, Some(START + 16));

        // mint an SBT to a user with same prefix as alice
        let minted_ids = ctr.sbt_mint(vec![(a_user(), vec![m1_1.clone()])]);
        assert_eq!(minted_ids, vec![1]);

        ctx.predecessor_account_id = issuer2();
        testing_env!(ctx.clone());
        let minted_ids = ctr.sbt_mint(vec![
            (alice(), vec![m1_1.clone()]),
            (bob(), vec![m1_2.clone()]),
            (a_user(), vec![m1_1.clone()]),
            (alice(), vec![m2_1.clone()]),
        ]);
        assert_eq!(minted_ids, vec![1, 2, 3, 4]);

        // mint again for Alice
        let minted_ids = ctr.sbt_mint(vec![(alice(), vec![m4_1.clone()])]);
        assert_eq!(minted_ids, vec![5]);

        // change the issuer and mint new tokens for alice
        ctx.predecessor_account_id = issuer3();
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

        assert_eq!(ctr.sbt_supply(issuer1()), 1);
        assert_eq!(ctr.sbt_supply(issuer2()), 5);
        assert_eq!(ctr.sbt_supply(issuer3()), 2);
        assert_eq!(ctr.sbt_supply(issuer4()), 0);

        assert_eq!(3, ctr.sbt_supply_by_owner(alice(), issuer2(), None));
        assert_eq!(2, ctr.sbt_supply_by_owner(alice(), issuer3(), None));
        assert_eq!(1, ctr.sbt_supply_by_owner(bob(), issuer2(), None));
        assert_eq!(0, ctr.sbt_supply_by_owner(bob(), issuer3(), None));
        assert_eq!(0, ctr.sbt_supply_by_owner(issuer2(), issuer2(), None));

        assert_eq!(
            ctr.sbt(issuer2(), 1).unwrap(),
            mk_token(1, alice(), m1_1.clone())
        );
        assert_eq!(
            ctr.sbt(issuer2(), 2).unwrap(),
            mk_token(2, bob(), m1_2.clone())
        );
        assert_eq!(
            ctr.sbt(issuer2(), 3).unwrap(),
            mk_token(3, a_user(), m1_1.clone())
        );
        assert_eq!(
            ctr.sbt(issuer2(), 4).unwrap(),
            mk_token(4, alice(), m2_1.clone())
        );
        assert_eq!(
            ctr.sbt(issuer3(), 1).unwrap(),
            mk_token(1, alice(), m1_1.clone())
        );

        // Token checks

        let a_tokens = vec![
            (issuer1(), vec![mk_owned_token(1, m1_1.clone())]),
            (issuer2(), vec![mk_owned_token(3, m1_1.clone())]),
        ];
        assert_eq!(
            &ctr.sbt_tokens_by_owner(a_user(), None, None, None),
            &a_tokens
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(a_user(), Some(issuer1()), None, None),
            vec![a_tokens[0].clone()],
        );
        assert_eq!(
            ctr.sbt_tokens_by_owner(a_user(), Some(issuer2()), None, None),
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
            vec![mk_owned_token(1, m1_1.clone()), mk_owned_token(2, m2_1)],
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
            vec![alice_issuer3]
        );
    }
}
