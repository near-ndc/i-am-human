use crate::*;

// community-open/v1.0.0 structs

#[derive(BorshDeserialize)]
pub struct OldContract {
    pub classes: LookupMap<ClassId, ClassMinters>,
    pub next_class: ClassId,
    pub registry: AccountId,
    pub metadata: LazyOption<ContractMetadata>,
    pub class_metadata: LookupMap<ClassId, ClassMetadata>,
}

// migration to community-open/v...
#[near_bindgen]
impl Contract {
    #[private]
    #[init(ignore_state)]
    pub fn migrate() -> Self {
        let old_state: OldContract = env::state_read().expect("can't deserialize contract");

        // changed fields:
        // -

        Self {
            classes: old_state.classes,
            next_class: old_state.next_class,
            registry: old_state.registry,
            metadata: old_state.metadata,
            class_metadata: old_state.class_metadata,
        }
    }
}
