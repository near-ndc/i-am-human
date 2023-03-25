mod events;
mod metadata;

use near_sdk::Balance;
use near_sdk::Gas;

pub use crate::events::*;
pub use crate::metadata::*;

// u64 capacity is more than 1e19. If we will mint 10'000 SBTs per second, than it will take us
// 58'494'241 years to get into the capacity.
// Today, the JS integer limit is `2^53-1 ~ 9e15`. It will take us 28'561 years to fill that when minting
// 10'000 SBTs per second.
// So, we don't need to u128 nor a String type.
pub type TokenId = u64;

/// This spec can be treated like a version of the standard.
pub const SPEC_VERSION: &str = "1.0.0";
/// This is the name of the SBT standard we're using
pub const STANDARD_NAME: &str = "nep393";

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
