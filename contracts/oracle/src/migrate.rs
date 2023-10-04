use crate::*;

// registry/v1.3.0
#[derive(BorshDeserialize, PanicOnDefault)]
pub struct OldState {
    pub metadata: LazyOption<ContractMetadata>,
    pub registry: AccountId,
    pub claim_ttl: u64,
    pub sbt_ttl_ms: u64,
    pub authority_pubkey: [u8; PUBLIC_KEY_LEN],
    pub used_identities: UnorderedSet<Vec<u8>>,
    pub admins: UnorderedSet<AccountId>,
}

#[near_bindgen]
impl Contract {
    #[private]
    #[init(ignore_state)]
    /* pub  */
    pub fn migrate(class_metadata: Vec<(ClassId, ClassMetadata)>) -> Self {
        let old_state: OldState = env::state_read().expect("failed");
        // new field in the smart contract :
        // + class_metadata: LookupMap<ClassId, ClassMetadata>

        let mut c_metadata = LookupMap::new(StorageKey::ClassMetadata);
        for (class_id, class_metadata) in class_metadata {
            c_metadata.insert(&class_id, &class_metadata);
        }

        Self {
            metadata: old_state.metadata,
            registry: old_state.registry,
            claim_ttl: old_state.claim_ttl,
            sbt_ttl_ms: old_state.sbt_ttl_ms,
            authority_pubkey: old_state.authority_pubkey,
            used_identities: old_state.used_identities,
            admins: old_state.admins,
            class_metadata: c_metadata,
        }
    }
}
