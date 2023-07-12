use crate::*;

#[derive(BorshDeserialize)]
pub struct OldContract {
    pub admin: AccountId,
    /// map of classId -> to set of accounts authorized to mint
    pub minting_authorities: LookupMap<ClassId, Vec<AccountId>>,

    pub registry: AccountId,
    pub metadata: LazyOption<ContractMetadata>,
    pub ttl: u64,
}

#[near_bindgen]
impl Contract {
    #[private]
    #[init(ignore_state)]
    pub fn migrate() -> Self {
        let mut old_state: OldContract = env::state_read().expect("failed");

        // changed fields:
        // next_class -- new field
        // minting_authorities: LookupMap<ClassId, Vec<AccountId>>
        //   -> LookupMap<ClassId, ClassMinter>,

        let mut new_minters: Vec<ClassMinter> = Vec::new();
        let mut next_class = 1;
        for i in 1..=3 {
            if let Some(minters) = old_state.minting_authorities.remove(&i) {
                next_class = i;
                new_minters.push(ClassMinter {
                    requires_iah: true,
                    minters,
                });
            }
        }
        // all classes, except the first one require IAH
        if !new_minters.is_empty() {
            new_minters[0].requires_iah = false;
        }

        let mut classes = LookupMap::new(StorageKey::MintingAuthority);
        for (idx, cm) in new_minters.iter().enumerate() {
            classes.insert(&(idx as u64 + 1), cm);
        }

        Self {
            admin: old_state.admin,
            classes,
            next_class,
            registry: old_state.registry,
            metadata: old_state.metadata,
            ttl: old_state.ttl,
        }
    }
}
