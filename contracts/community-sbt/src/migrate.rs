use crate::*;

// community-sbt/v4.1.0 old structs

#[derive(BorshDeserialize)]
pub struct OldContract {
    pub admin: AccountId,
    pub classes: LookupMap<ClassId, OldClassMinters>,
    pub next_class: ClassId,
    pub registry: AccountId,
    pub metadata: LazyOption<ContractMetadata>,
    pub ttl: u64,
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct OldClassMinters {
    pub requires_iah: bool,
    pub minters: Vec<AccountId>,
}

// migration to community-sbt/v4.2.0
#[near_bindgen]
impl Contract {
    #[private]
    #[init(ignore_state)]
    pub fn migrate() -> Self {
        let mut old_state: OldContract = env::state_read().expect("can't deserialize contract");

        // changed fields:
        // ttl -- removed
        // classes: LookupMap<ClassId, OldClassMinters>,
        //   changed to ->  LookupMap<ClassId, ClassMinters>, where ClassMinters has a new field: max_ttl:u64,

        let mut classes = LookupMap::new(StorageKey::MintingAuthority);
        let max_ttl = old_state.ttl;
        for i in 1..=5 {
            if let Some(minters) = old_state.classes.remove(&i) {
                classes.insert(
                    &i,
                    &ClassMinters {
                        requires_iah: minters.requires_iah,
                        minters: minters.minters,
                        max_ttl,
                    },
                );
            }
        }

        Self {
            admin: old_state.admin,
            classes,
            next_class: old_state.next_class,
            registry: old_state.registry,
            metadata: old_state.metadata,
            class_metadata: LookupMap::new(StorageKey::ClassMetadata),
        }
    }
}
