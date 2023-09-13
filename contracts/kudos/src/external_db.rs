use near_sdk::serde::Serialize;
use near_sdk::serde_json::Value;
use near_sdk::{ext_contract, AccountId, Promise, PromiseOrValue, PublicKey};

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct GetOptions {
    pub with_block_height: Option<bool>,
    pub with_node_id: Option<bool>,
    pub return_deleted: Option<bool>,
}

#[allow(unused)]
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub enum KeysReturnType {
    True,
    BlockHeight,
    NodeId,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct KeysOptions {
    pub return_type: Option<KeysReturnType>,
    pub return_deleted: Option<bool>,
}

#[ext_contract(ext_db)]
pub trait DatabaseProvider {
    fn set(&mut self, data: Value) -> Result<Promise, &'static str>;

    fn get(&self, keys: Vec<String>, options: Option<GetOptions>) -> PromiseOrValue<Value>;

    fn keys(&self, keys: Vec<String>, options: Option<KeysOptions>) -> PromiseOrValue<Value>;

    fn grant_write_permission(
        &mut self,
        predecessor_id: Option<AccountId>,
        public_key: Option<PublicKey>,
        keys: Vec<String>,
    ) -> Result<Promise, &'static str>;
}
