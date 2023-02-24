mod events;
mod metadata;

use near_sdk::Balance;
use near_sdk::Gas;

pub use crate::events::*;
pub use crate::metadata::*;

pub type TokenId = u64;

/// This spec can be treated like a version of the standard.
pub const METADATA_SPEC: &str = "1.0.0";
/// This is the name of the SBT standard we're using
pub const SBT_STANDARD_NAME: &str = "nep-393";

/// Balance of one mili NEAR, which is 10^23 Yocto NEAR.
pub const MILI_NEAR: Balance = 1_000_000_000_000_000_000_000;

pub const BLACKLIST_COST: Balance = 5 * MILI_NEAR;
pub const GAS_FOR_BLACKLIST: Gas = Gas(6 * Gas::ONE_TERA.0);

use near_sdk::{ext_contract, AccountId};

/// SBT Registry trait
#[ext_contract(ext_registry)]
pub trait SBTRegistry {
    fn blacklist(&mut self, account: AccountId, memo: Option<String>);
}
