use near_sdk::{Balance, Gas};

pub const MICRO_NEAR: Balance = 1_000_000_000_000_000_000; // 1e19 yoctoNEAR
pub const MILI_NEAR: Balance = 1_000 * MICRO_NEAR;

/// 1s in nano seconds.
pub const SECOND: u64 = 1_000_000_000;
/// 1ms in nano seconds.
pub const MSECOND: u64 = 1_000_000;

pub const GAS_NOMINATE: Gas = Gas(20 * Gas::ONE_TERA.0);
pub const GAS_UPVOTE: Gas = Gas(20 * Gas::ONE_TERA.0);
pub const GAS_COMMENT: Gas = Gas(20 * Gas::ONE_TERA.0);

/// nomination: (accountID, HouseType) -> (25 bytes  + 24 bytes) = 49 bytes < 100 bytes
pub const NOMINATE_COST: Balance = MILI_NEAR;

/// upvote: (accountID, Account) -> (25 bytes  + 25 bytes) = 50 bytes
/// upvotes_per_candidate: (accountID, u32) -> (25 bytes + 4 bytes) = 29 bytes
/// sum = 50 + 29 = 79 bytes < 100 bytes
pub const UPVOTE_COST: Balance = MILI_NEAR;

pub const MAX_CAMPAIGN_LEN: usize = 200;
