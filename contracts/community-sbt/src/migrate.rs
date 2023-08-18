use crate::*;

/*
// community-sbt/v4.1.0
#[derive(BorshDeserialize)]
pub struct OldContract {
    pub admin: AccountId,
    pub classes: LookupMap<ClassId, ClassMinters>,
    pub next_class: ClassId,
    pub registry: AccountId,
    pub metadata: LazyOption<ContractMetadata>,
}
*/

#[near_bindgen]
impl Contract {
    #[private]
    #[init(ignore_state)]
    pub fn migrate() -> Self {
        env::panic_str("not available in this upgrade");

        /*
        let mut old_state: OldContract = env::state_read().expect("failed");

        // changed fields:

        let mut classes = LookupMap::new(StorageKey::MintingAuthority);
        let max_ttl = old_state.ttl;
        for i in 1..=3 {
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
            metadata_class: LookupMap::new(StorageKey::ClassMetadata);
        }
         */
    }
}
