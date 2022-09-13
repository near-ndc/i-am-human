use near_sdk::{ext_contract, AccountId};

#[ext_contract(ext_blacklist)]
pub trait BlacklistAddress {
    fn blacklist(&mut self, account: AccountId, memo: Option<String>);
}
