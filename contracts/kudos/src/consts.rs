use near_sdk::env::STORAGE_PRICE_PER_BYTE;
use near_sdk::{Balance, Gas, StorageUsage};

pub(crate) const U64_STORAGE: StorageUsage = 8;
pub(crate) const U8_STORAGE: StorageUsage = 1;

/// Every contract storage key/value entry always uses 40 bytes when stored via `env::storage_write`
/// - key len as u64,
/// - key ptr as u64,
/// - value len as u64,
/// - value ptr as u64,
/// - register as u64
pub(crate) const STORAGE_ENTRY: StorageUsage = 5 * U64_STORAGE;

/// enum::StorageKey size [1 byte]
const ENUM_STORAGE_KEY: StorageUsage = U8_STORAGE;

/// Internal class id of ProofOfKudos SBT used by i-am-human-registry for this smart contract
pub const PROOF_OF_KUDOS_SBT_CLASS_ID: u64 = 1;

/// Required Ⓝ deposit to mint ProofOfKudos SBT by i-am-human-registry smart contract
///
/// This value should be updated if mentioned contract will require different amount of deposit
pub const PROOF_OF_KUDOS_SBT_MINT_COST: Balance = 9_000_000_000_000_000_000_000;

/// Required storage to memorise exchanged [`KudosId`] in [`LookupSet`] of this smart contract storage
///
/// The [`KudosId`] is represented by [`u64`] value which is serialized to 8 bytes plus [`STORAGE_ENTRY`] required
/// to store anything in contract storage
pub const EXCHANGE_KUDOS_STORAGE: StorageUsage = STORAGE_ENTRY + ENUM_STORAGE_KEY + U64_STORAGE;

/// Deposit required to exchange upvoted Kudos for ProofOfKudos SBT
///
/// This value includes a storage amount required to memorise exchanged [`KudosId`] and
/// the minting cost of a ProofOfKudos SBT by i-am-human-registry smart contract. This value
/// should be changed if any of the above will be changed
pub const EXCHANGE_KUDOS_COST: Balance =
    EXCHANGE_KUDOS_STORAGE as Balance * STORAGE_PRICE_PER_BYTE + PROOF_OF_KUDOS_SBT_MINT_COST;

/// Required storage for this contract registered as user at SocialDB to grant write permission to IAH Registry contract
///
/// This value was pre-computed by using maximum (64 characters) account id length for IAH Registry and this contracts.
pub const SOCIAL_DB_GRANT_WRITE_PERMISSION_COST: Balance = 3_100_000_000_000_000_000_000;

/// Deposit required to give kudos to user.
///
/// The computed deposit amount is based on a case when user with maximum account name length (64 characters)
/// grants kudos with maximum provided commentary text length (1000 escaped ascii characters),
/// icon CID for ipfs and maximum number of allowed hashtags (10) with a hashtag of maximum
/// text length (limited to 32 characters, and allows to use only alphanumeric ascii characters, underscores and gyphens)
/// to a user with maximum account name length (64 characters). The exact value of this computation is 0.0961 Ⓝ and
/// it is rounded to 0.1 Ⓝ. This value should be recomputed if the above restrications will be changed.
pub const GIVE_KUDOS_COST: Balance = 100_000_000_000_000_000_000_000; // 0.1 Ⓝ (0.09802)

/// Deposit required to leave a commentary message for kudos
///
/// The computed deposit amount is based on a case when user with maximum account name length (64 characters)
/// leaves a commentary message text with maximum length (1000 escaped ascii characters)
/// to a user with maximum account name length (64 characters). The exact value of this computation when
/// no parent commentary id provided is 0.01653 Ⓝ and it is rounded to 0.017 Ⓝ. This value should be recomputed
/// if the above restrications will be changed.
pub const LEAVE_COMMENT_COST: Balance = 17_000_000_000_000_000_000_000; // 0.017 Ⓝ (0.01653 when no parent commentary id)

/// Deposit required to upvote kudos
///
/// The computed deposit amount is based on a case when user with maximum account name length (64 characters)
/// upvotes kudos. The exact value of this computation is 0.00311 Ⓝ and it is rounded to 0.004 Ⓝ.
/// This value should be recomputed if the above restrications will be changed.
pub const UPVOTE_KUDOS_COST: Balance = 4_000_000_000_000_000_000_000; // 0.004 Ⓝ (0.00311)

/// Gas reserved for final failure callback which panics if one of the callback fails.
pub const FAILURE_CALLBACK_GAS: Gas = Gas(5 * Gas::ONE_TERA.0);

/// Gas required for a [`save_kudos`](kudos_contract::callbacks::give_kudos::ContractExt::save_kudos) callback
pub const SAVE_KUDOS_RESERVED_GAS: Gas = Gas(15 * Gas::ONE_TERA.0);
/// Gas required for a [`on_kudos_saved`](kudos_contract::callbacks::give_kudos::ContractExt::on_kudos_saved) callback
pub const KUDOS_SAVED_CALLBACK_GAS: Gas = Gas(10 * Gas::ONE_TERA.0);
/// Gas reserved to a public method [`give_kudos`](kudos_contract::public::Contract::give_kudos)
pub const GIVE_KUDOS_RESERVED_GAS: Gas = Gas(15 * Gas::ONE_TERA.0);

/// Gas required for a [`acquire_kudos_sender`](kudos_contract::callbacks::upvote_kudos::ContractExt::acquire_kudos_sender) callback
pub const ACQUIRE_KUDOS_SENDER_RESERVED_GAS: Gas = Gas(15 * Gas::ONE_TERA.0);
/// Gas required for a [`on_kudos_sender_acquired`](kudos_contract::callbacks::upvote_kudos::ContractExt::on_kudos_sender_acquired) callback
pub const KUDOS_SENDER_ACQUIRED_CALLBACK_GAS: Gas = Gas(15 * Gas::ONE_TERA.0);
/// Gas required for a [`on_kudos_upvote_saved`](kudos_contract::callbacks::upvote_kudos::ContractExt::on_kudos_upvote_saved) callback
pub const KUDOS_UPVOTE_SAVED_CALLBACK_GAS: Gas = Gas(10 * Gas::ONE_TERA.0);
/// Gas reserved to a public method [`upvote_kudos`](kudos_contract::public::Contract::upvote_kudos)
pub const UPVOTE_KUDOS_RESERVED_GAS: Gas = Gas(15 * Gas::ONE_TERA.0);

/// Gas required for a [`acquire_kudos_info`](kudos_contract::callbacks::leave_comment::ContractExt::acquire_kudos_info) callback
pub const ACQUIRE_KUDOS_INFO_RESERVED_GAS: Gas = Gas(15 * Gas::ONE_TERA.0);
/// Gas required for a [`on_kudos_info_acquired`](kudos_contract::callbacks::leave_comment::ContractExt::on_kudos_info_acquired) callback
pub const KUDOS_INFO_ACQUIRED_CALLBACK_GAS: Gas = Gas(15 * Gas::ONE_TERA.0);
/// Gas required for a [`on_commentary_saved`](kudos_contract::callbacks::leave_comment::ContractExt::on_commentary_saved) callback
pub const KUDOS_COMMENT_SAVED_CALLBACK_GAS: Gas = Gas(10 * Gas::ONE_TERA.0);
/// Gas reserved to a public method [`leave_comment`](kudos_contract::public::Contract::leave_comment)
pub const LEAVE_COMMENT_RESERVED_GAS: Gas = Gas(15 * Gas::ONE_TERA.0);

/// Gas required for a [`acquire_number_of_upvotes`](kudos_contract::callbacks::exchange_kudos_for_sbt::ContractExt::acquire_number_of_upvotes) callback
pub const ACQUIRE_NUMBER_OF_UPVOTES_RESERVED_GAS: Gas = Gas(15 * Gas::ONE_TERA.0);
/// Gas required for a [`on_kudos_upvotes_acquired`](kudos_contract::callbacks::exchange_kudos_for_sbt::ContractExt::on_kudos_upvotes_acquired) callback
pub const KUDOS_UPVOTES_ACQUIRED_CALLBACK_GAS: Gas = Gas(10 * Gas::ONE_TERA.0);
/// Gas required for a [`sbt_mint`](kudos_contract::registry::ExtSbtRegistry::sbt_mint) cross contract call
pub const PROOF_OF_KUDOS_SBT_MINT_GAS: Gas = Gas(10 * Gas::ONE_TERA.0);
/// Gas required for a [`on_pok_sbt_mint`](kudos_contract::callbacks::exchange_kudos_for_sbt::ContractExt::on_pok_sbt_mint) callback
pub const PROOF_OF_KUDOS_SBT_MINT_CALLBACK_GAS: Gas = Gas(10 * Gas::ONE_TERA.0);
/// Gas reserved to a public method [`exchange_kudos_for_sbt`](kudos_contract::public::Contract::exchange_kudos_for_sbt)
pub const EXCHANGE_KUDOS_FOR_SBT_RESERVED_GAS: Gas = Gas(15 * Gas::ONE_TERA.0);

/// Gas required minimum for `get` and `set` methods of NEAR social db smart contract.
///
/// All remainder gas will be passed additionally for these calls.
pub const SOCIAL_DB_REQUEST_MIN_RESERVED_GAS: Gas = Gas(10 * Gas::ONE_TERA.0);
