use near_sdk::{Balance, Gas};

pub const MICRO_NEAR: Balance = 1_000_000_000_000_000_000;
pub const MILI_NEAR: Balance = 1000 * MICRO_NEAR;

pub const MINT_COST: Balance = 9 * MILI_NEAR; // 0.009 NEAR. Storage cost is 0.843N from running contract
pub const BAN_COST: Balance = 5 * MILI_NEAR;

pub const MINT_GAS: Gas = Gas(7 * Gas::ONE_TERA.0);
pub const IS_HUMAN_GAS: Gas = Gas(2 * Gas::ONE_TERA.0);
pub const BLACKLIST_GAS: Gas = Gas(6 * Gas::ONE_TERA.0);

/// calculates amount of gas required by registry for `sbt_renew` call.
#[inline]
pub const fn renew_gas(num_tokens: usize) -> Gas {
    // 2tera + num_tokens * 0.2tera * num_tokens
    Gas(2 * Gas::ONE_TERA.0 + num_tokens as u64 * 200_000_000_000)
}

pub const fn calculate_mint_gas(num_tokens: usize) -> Gas {
    Gas((num_tokens as u64 + 1) * MINT_GAS.0)
}

pub const fn calculate_iah_mint_gas(num_tokens: usize, accounts: usize) -> Gas {
    Gas(calculate_mint_gas(num_tokens).0 + accounts as u64 * IS_HUMAN_GAS.0)
}

pub const fn mint_deposit(num_tokens: usize) -> Balance {
    num_tokens as u128 * MINT_COST
}
