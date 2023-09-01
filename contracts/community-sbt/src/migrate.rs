use crate::*;

// community-sbt/v4.1.0 old structs

#[derive(BorshDeserialize)]
pub struct OldContract {
    pub admin: AccountId,
    pub classes: LookupMap<ClassId, ClassMinters>,
    pub next_class: ClassId,
    pub registry: AccountId,
    pub metadata: LazyOption<ContractMetadata>,
    pub class_metadata: LookupMap<ClassId, ClassMetadata>,
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
        let old_state: OldContract = env::state_read().expect("can't deserialize contract");

        // changed fields:
        // ttl -- removed
        // pub admin: AccountId,
        //   changed to ->  pub admins: LookupSet<AccountId>,

        let mut admins = LookupSet::new(StorageKey::Admins);
        admins.insert(&old_state.admin);

        Self {
            admins: admins,
            classes: old_state.classes,
            next_class: old_state.next_class,
            registry: old_state.registry,
            metadata: old_state.metadata,
            class_metadata: old_state.class_metadata,
        }
    }
}
