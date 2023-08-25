use crate::consts::*;
use crate::external_db::ext_db;
use crate::registry::TokenId;
use crate::types::KudosId;
use crate::utils::*;
use crate::{Contract, ContractExt};
use near_sdk::json_types::U128;
use near_sdk::json_types::U64;
use near_sdk::serde_json::Value;
use near_sdk::{env, near_bindgen, AccountId, Promise, PromiseError, PromiseOrValue};

#[near_bindgen]
impl Contract {
    #[private]
    pub fn acquire_kudos_sender(
        &mut self,
        predecessor_account_id: AccountId,
        attached_deposit: U128,
        external_db_id: AccountId,
        receiver_id: AccountId,
        kudos_id: KudosId,
        #[callback_result] callback_result: Result<Vec<(AccountId, Vec<TokenId>)>, PromiseError>,
    ) -> Promise {
        let attached_deposit = attached_deposit.0;

        let result = callback_result
            .map_err(|e| format!("IAHRegistry::is_human() call failure: {e:?}"))
            .and_then(|tokens| {
                if tokens.is_empty() {
                    return Err("IAHRegistry::is_human() returns result: Not a human".to_owned());
                }

                let root_id = env::current_account_id();
                let sender_id = env::signer_account_id();
                let upvote_kudos_req =
                    build_upvote_kudos_request(&root_id, &sender_id, &receiver_id, &kudos_id)?;
                let get_kudos_by_id_req =
                    build_get_kudos_by_id_request(&root_id, &receiver_id, &kudos_id);

                // Compute minimum required gas and split the remaining gas by two equal parts for
                // NEAR Social db subsequent calls
                let get_kudos_by_id_gas = (env::prepaid_gas()
                    - (ACQUIRE_KUDOS_SENDER_RESERVED_GAS
                        + KUDOS_SENDER_ACQUIRED_CALLBACK_GAS
                        + KUDOS_UPVOTE_SAVED_CALLBACK_GAS
                        + FAILURE_CALLBACK_GAS))
                    / 2;
                let get_kudos_by_id_callback_gas = get_kudos_by_id_gas
                    + KUDOS_SENDER_ACQUIRED_CALLBACK_GAS
                    + KUDOS_UPVOTE_SAVED_CALLBACK_GAS
                    + FAILURE_CALLBACK_GAS;

                Ok(ext_db::ext(external_db_id.clone())
                    .with_static_gas(get_kudos_by_id_gas)
                    .get(vec![get_kudos_by_id_req.clone()], None)
                    .then(
                        Self::ext(env::current_account_id())
                            .with_static_gas(get_kudos_by_id_callback_gas)
                            .on_kudos_sender_acquired(
                                predecessor_account_id.clone(),
                                attached_deposit.into(),
                                external_db_id,
                                get_kudos_by_id_req,
                                upvote_kudos_req,
                            ),
                    ))
            });

        result.unwrap_or_else(|e| {
            Promise::new(predecessor_account_id)
                .transfer(attached_deposit)
                .then(
                    Self::ext(env::current_account_id())
                        .with_static_gas(FAILURE_CALLBACK_GAS)
                        .on_failure(e),
                )
        })
    }

    #[private]
    pub fn on_kudos_sender_acquired(
        &mut self,
        predecessor_account_id: AccountId,
        attached_deposit: U128,
        external_db_id: AccountId,
        get_kudos_by_id_req: String,
        upvote_kudos_req: Value,
        #[callback_result] callback_result: Result<Value, PromiseError>,
    ) -> Promise {
        let attached_deposit = attached_deposit.0;

        let Err(e) = callback_result
            .map_err(|e| format!("SocialDB::get({get_kudos_by_id_req}) call failure: {e:?}"))
            .and_then(|mut kudos_by_id_res| {
                match extract_kudos_id_sender_from_response(&get_kudos_by_id_req, &mut kudos_by_id_res) {
                    Some(sender_id) if sender_id == env::signer_account_id() => {
                        Err("User is not eligible to upvote this kudos".to_owned())
                    }
                    Some(_) => Ok(()),
                    None => Err("Unable to acquire a Kudos sender account id".to_owned())
                }
            }) else {
                let gas_left = env::prepaid_gas()
                    - (KUDOS_SENDER_ACQUIRED_CALLBACK_GAS
                        + KUDOS_UPVOTE_SAVED_CALLBACK_GAS
                        + FAILURE_CALLBACK_GAS);

                return ext_db::ext(external_db_id)
                    .with_attached_deposit(attached_deposit)
                    .with_static_gas(gas_left)
                    .set(upvote_kudos_req)
                    .then(
                        Self::ext(env::current_account_id())
                            .with_static_gas(KUDOS_UPVOTE_SAVED_CALLBACK_GAS + FAILURE_CALLBACK_GAS)
                            .on_kudos_upvote_saved(
                                predecessor_account_id,
                                attached_deposit.into(),
                            ),
                    );
            };

        // Return upvote kudos deposit back to sender if failed
        Promise::new(predecessor_account_id)
            .transfer(attached_deposit)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(FAILURE_CALLBACK_GAS)
                    .on_failure(e),
            )
    }

    #[private]
    pub fn on_kudos_upvote_saved(
        &mut self,
        predecessor_account_id: AccountId,
        attached_deposit: U128,
        #[callback_result] callback_result: Result<(), PromiseError>,
    ) -> PromiseOrValue<U64> {
        let attached_deposit = attached_deposit.0;

        match callback_result {
            Ok(_) => PromiseOrValue::Value(env::block_timestamp_ms().into()),
            Err(e) => {
                // Return deposit back to sender if NEAR SocialDb write failure
                Promise::new(predecessor_account_id)
                    .transfer(attached_deposit)
                    .then(
                        Self::ext(env::current_account_id())
                            .with_static_gas(FAILURE_CALLBACK_GAS)
                            .on_failure(format!("SocialDB::set() call failure: {e:?}")),
                    )
                    .into()
            }
        }
    }
}
