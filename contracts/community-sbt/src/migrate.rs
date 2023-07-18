use crate::*;

/// Helper structure for keys of the persistent collections.
#[derive(BorshDeserialize, BorshSerialize)]
pub struct OldClassMinters {
    /// if true only iah verifed accounts can obrain the SBT
    pub requires_iah: bool,
    /// accounts allowed to mint the SBT
    pub minters: Vec<AccountId>,
}

#[derive(BorshDeserialize)]
pub struct OldContract {
    /// Account authorized to add new minting authority
    pub admin: AccountId,
    /// map of classId -> to set of accounts authorized to mint
    pub classes: LookupMap<ClassId, OldClassMinters>,
    pub next_class: ClassId,

    /// SBT registry.
    pub registry: AccountId,
    /// contract metadata
    pub metadata: LazyOption<ContractMetadata>,
    /// time to live in ms. Overwrites metadata.expire_at.
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

        let mut classes = LookupMap::new(StorageKey::MintingAuthority);
        let ttl = old_state.ttl;
        for i in 1..=3 {
            if let Some(minters) = old_state.classes.remove(&i) {
                classes.insert(
                    &i,
                    &ClassMinters {
                        requires_iah: minters.requires_iah,
                        minters: minters.minters,
                        ttl,
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
        }
    }
}
