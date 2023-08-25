use crate::external_db::ext_db;
use crate::registry::TokenId;
use crate::types::{CommentId, KudosId};
use crate::utils::*;
use crate::{consts::*, EncodedCommentary};
use crate::{Contract, ContractExt};
use near_sdk::json_types::U128;
use near_sdk::serde_json::Value;
use near_sdk::{env, near_bindgen, AccountId, Promise, PromiseError, PromiseOrValue};

#[near_bindgen]
impl Contract {
    #[private]
    pub fn acquire_kudos_info(
        &mut self,
        predecessor_account_id: AccountId,
        attached_deposit: U128,
        external_db_id: AccountId,
        receiver_id: AccountId,
        kudos_id: KudosId,
        parent_comment_id: Option<CommentId>,
        comment: EncodedCommentary,
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
                let comment_id = CommentId::from(self.last_incremental_id.inc());
                let leave_comment_req = build_leave_comment_request(
                    &root_id,
                    &receiver_id,
                    &kudos_id,
                    &comment_id,
                    &comment,
                )?;
                let get_kudos_by_id_req =
                    build_get_kudos_by_id_request(&root_id, &receiver_id, &kudos_id);
                let mut get_req = vec![get_kudos_by_id_req.clone()];

                if let Some(comment_id) = parent_comment_id.as_ref() {
                    get_req.push(build_get_kudos_comment_by_id_request(
                        &root_id,
                        &receiver_id,
                        &kudos_id,
                        comment_id,
                    ));
                }

                // Compute minimum required gas and split the remaining gas by two equal parts for
                // NEAR Social db subsequent calls
                let get_kudos_by_id_gas = (env::prepaid_gas()
                    - (ACQUIRE_KUDOS_INFO_RESERVED_GAS
                        + KUDOS_INFO_ACQUIRED_CALLBACK_GAS
                        + KUDOS_COMMENT_SAVED_CALLBACK_GAS
                        + FAILURE_CALLBACK_GAS))
                    / 2;
                let get_kudos_by_id_callback_gas = get_kudos_by_id_gas
                    + KUDOS_INFO_ACQUIRED_CALLBACK_GAS
                    + KUDOS_COMMENT_SAVED_CALLBACK_GAS
                    + FAILURE_CALLBACK_GAS;

                Ok(ext_db::ext(external_db_id.clone())
                    .with_static_gas(get_kudos_by_id_gas)
                    .get(get_req, None)
                    .then(
                        Self::ext(env::current_account_id())
                            .with_static_gas(get_kudos_by_id_callback_gas)
                            .on_kudos_info_acquired(
                                predecessor_account_id.clone(),
                                attached_deposit.into(),
                                external_db_id,
                                get_kudos_by_id_req,
                                leave_comment_req,
                                parent_comment_id,
                                comment_id,
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
    pub fn on_kudos_info_acquired(
        &mut self,
        predecessor_account_id: AccountId,
        attached_deposit: U128,
        external_db_id: AccountId,
        get_kudos_by_id_req: String,
        leave_comment_req: Value,
        parent_comment_id: Option<CommentId>,
        comment_id: CommentId,
        #[callback_result] callback_result: Result<Value, PromiseError>,
    ) -> Promise {
        let attached_deposit = attached_deposit.0;

        let Err(e) = callback_result
            .map_err(|e| {
                format!(
                    "SocialDB::get({get_kudos_by_id_req}) call failure: {e:?}"
                )
            })
            .and_then(|mut kudos_by_id_res| {
                if let Some(comment_id) = parent_comment_id.as_ref() {
                    // We do not verify if extracted base64-encoded commentary is valid, we assume 
                    // that data stored in social db is not corrupted.
                    let _ = extract_kudos_encoded_comment_by_id_from_response(&get_kudos_by_id_req, comment_id, &mut kudos_by_id_res)
                        .ok_or_else(|| {
                            "Unable to verify parent commentary id".to_owned()
                        })?;
                }

                extract_kudos_id_sender_from_response(&get_kudos_by_id_req, &mut kudos_by_id_res)
                    .ok_or_else(|| {
                        "Unable to acquire a Kudos sender account id".to_owned()
                    })
            }) else {
                let gas_left = env::prepaid_gas()
                    - (KUDOS_INFO_ACQUIRED_CALLBACK_GAS + KUDOS_COMMENT_SAVED_CALLBACK_GAS + FAILURE_CALLBACK_GAS);

                return ext_db::ext(external_db_id)
                    .with_attached_deposit(attached_deposit)
                    .with_static_gas(gas_left)
                    .set(leave_comment_req)
                    .then(
                        Self::ext(env::current_account_id())
                            .with_static_gas(KUDOS_COMMENT_SAVED_CALLBACK_GAS + FAILURE_CALLBACK_GAS)
                            .on_commentary_saved(
                                predecessor_account_id,
                                attached_deposit.into(),
                                comment_id,
                            ),
                    );
            };

        // Return leave comment deposit back to sender if failed
        Promise::new(predecessor_account_id)
            .transfer(attached_deposit)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(FAILURE_CALLBACK_GAS)
                    .on_failure(e),
            )
    }

    #[private]
    pub fn on_commentary_saved(
        &mut self,
        predecessor_account_id: AccountId,
        attached_deposit: U128,
        comment_id: CommentId,
        #[callback_result] callback_result: Result<(), PromiseError>,
    ) -> PromiseOrValue<CommentId> {
        let attached_deposit = attached_deposit.0;

        match callback_result {
            Ok(_) => PromiseOrValue::Value(comment_id),
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
