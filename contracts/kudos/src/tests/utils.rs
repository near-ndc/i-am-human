use near_sdk::PromiseOrValue;
use near_sdk::{test_utils::VMContextBuilder, AccountId, Balance, Gas};

pub const MAX_GAS: Gas = Gas(300_000_000_000_000);

pub fn build_default_context(
    predecessor_account_id: AccountId,
    deposit: Option<Balance>,
    prepaid_gas: Option<Gas>,
) -> VMContextBuilder {
    let mut builder = VMContextBuilder::new();
    builder
        .signer_account_id(predecessor_account_id.clone())
        .predecessor_account_id(predecessor_account_id)
        .prepaid_gas(prepaid_gas.unwrap_or(MAX_GAS))
        .attached_deposit(deposit.unwrap_or_default());
    builder
}

pub fn promise_or_value_result_into_result<T: std::fmt::Debug>(
    value: Result<PromiseOrValue<T>, &'static str>,
) -> Result<String, String> {
    match value {
        Ok(PromiseOrValue::Promise(_)) => Ok("Promise".to_owned()),
        Ok(PromiseOrValue::Value(res)) => Ok(format!("{res:?}")),
        Err(e) => Err(e.to_owned()),
    }
}
