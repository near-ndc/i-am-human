use crate::*;

#[derive(BorshDeserialize)]
pub struct OldContract {
    /// Account authorized to add new minting authority
    pub admin: AccountId,
    /// map of classId -> to set of accounts authorized to mint
    pub classes: LookupMap<ClassId, ClassMinters>,
    pub next_class: ClassId,
    /// SBT registry.
    pub registry: AccountId,
    /// contract metadata
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
        // ttl -- removed
        // classes: LookupMap<ClassId, ClassMinters>,
        //   -> LookupMap<ClassId, ClassMinters>, where ClassMinters has a new field: ttl:u64,

        let mut new_minters: Vec<ClassMinters> = Vec::new();
        let mut next_class = 1;
        for i in 1..=3 {
            if let Some(minters) = old_state.classes.remove(&i) {
                next_class = i;
                new_minters.push(ClassMinters {
                    requires_iah: minters.requires_iah,
                    /// accounts allowed to mint the SBT
                    minters: minters.minters,
                    /// time to live in ms. Overwrites metadata.expire_at.
                    ttl: old_state.ttl,
                });
            }
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
        }
    }
}
