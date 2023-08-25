use super::utils::parse_kudos_and_verify_if_allowed_to_exchange;
use crate::consts::*;
use crate::external_db::ext_db;
use crate::registry::{ext_sbtreg, TokenId};
use crate::settings::Settings;
use crate::types::KudosId;
use crate::utils::*;
use crate::{Contract, ContractExt};
use near_sdk::json_types::U128;
use near_sdk::serde_json::Value;
use near_sdk::{env, near_bindgen, AccountId, Promise, PromiseError, PromiseOrValue};

#[near_bindgen]
impl Contract {
    #[private]
    pub fn acquire_number_of_upvotes(
        &mut self,
        predecessor_account_id: AccountId,
        attached_deposit: U128,
        external_db_id: AccountId,
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

                let receiver_id = env::signer_account_id();
                let root_id = env::current_account_id();
                let kudos_upvotes_path =
                    build_kudos_upvotes_path(&root_id, &receiver_id, &kudos_id);
                let acquire_upvotes_req = [&kudos_upvotes_path, "/*"].concat();
                let kudos_kind_path = build_kudos_kind_path(&root_id, &receiver_id, &kudos_id);

                let upvotes_acquired_callback_gas = KUDOS_UPVOTES_ACQUIRED_CALLBACK_GAS
                    + PROOF_OF_KUDOS_SBT_MINT_GAS
                    + PROOF_OF_KUDOS_SBT_MINT_CALLBACK_GAS
                    + FAILURE_CALLBACK_GAS;

                let acquire_upvotes_gas = env::prepaid_gas()
                    - (ACQUIRE_NUMBER_OF_UPVOTES_RESERVED_GAS + upvotes_acquired_callback_gas);

                Ok(ext_db::ext(external_db_id)
                    .with_static_gas(acquire_upvotes_gas)
                    .get(vec![acquire_upvotes_req, kudos_kind_path.clone()], None)
                    .then(
                        Self::ext(env::current_account_id())
                            .with_static_gas(upvotes_acquired_callback_gas)
                            .on_kudos_upvotes_acquired(
                                predecessor_account_id.clone(),
                                attached_deposit.into(),
                                kudos_id,
                                kudos_upvotes_path,
                                kudos_kind_path,
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
    pub fn on_kudos_upvotes_acquired(
        &mut self,
        predecessor_account_id: AccountId,
        attached_deposit: U128,
        kudos_id: KudosId,
        kudos_upvotes_path: String,
        kudos_kind_path: String,
        #[callback_result] kudos_result: Result<Value, PromiseError>,
    ) -> Promise {
        let attached_deposit = attached_deposit.0;
        let settings = Settings::from(&self.settings);

        match parse_kudos_and_verify_if_allowed_to_exchange(
            kudos_result,
            kudos_upvotes_path,
            kudos_kind_path,
            settings.min_number_of_upvotes_to_exchange_kudos as usize,
        )
        .and_then(|_| {
            let issued_at = env::block_timestamp_ms();
            let expires_at = settings.acquire_pok_sbt_expire_at_ts(issued_at)?;

            Ok(build_pok_sbt_metadata(issued_at, expires_at))
        }) {
            Ok(metadata) => {
                self.exchanged_kudos.insert(kudos_id.clone());

                ext_sbtreg::ext(self.iah_registry.clone())
                    .with_attached_deposit(PROOF_OF_KUDOS_SBT_MINT_COST)
                    .with_static_gas(PROOF_OF_KUDOS_SBT_MINT_GAS)
                    .sbt_mint(vec![(env::signer_account_id(), vec![metadata])])
                    .then(
                        Self::ext(env::current_account_id())
                            .with_static_gas(
                                PROOF_OF_KUDOS_SBT_MINT_CALLBACK_GAS + FAILURE_CALLBACK_GAS,
                            )
                            .on_pok_sbt_mint(
                                predecessor_account_id,
                                attached_deposit.into(),
                                kudos_id,
                            ),
                    )
            }
            Err(e) => {
                // Return leave comment deposit back to sender if failed
                Promise::new(predecessor_account_id)
                    .transfer(attached_deposit)
                    .then(
                        Self::ext(env::current_account_id())
                            .with_static_gas(FAILURE_CALLBACK_GAS)
                            .on_failure(e),
                    )
            }
        }
    }

    #[private]
    #[handle_result]
    pub fn on_pok_sbt_mint(
        &mut self,
        predecessor_account_id: AccountId,
        attached_deposit: U128,
        kudos_id: KudosId,
        #[callback_result] callback_result: Result<Vec<u64>, PromiseError>,
    ) -> Result<PromiseOrValue<Vec<u64>>, &'static str> {
        let attached_deposit = attached_deposit.0;

        match callback_result {
            Ok(minted_tokens_ids) if minted_tokens_ids.is_empty() => {
                // If IAHRegistry contract succeeds but returns an empty tokens list,
                // we treat is an unexpected failure and panic. No user deposit returns for this case.
                Err("IAHRegistry::sbt_mint() responses with an empty tokens array")
            }
            Ok(minted_tokens_ids) => Ok(PromiseOrValue::Value(minted_tokens_ids)),
            Err(e) => {
                // If tokens weren't minted, remove kudos from exchanged table
                self.exchanged_kudos.remove(&kudos_id);

                // Return deposit back to sender if IAHRegistry::sbt_mint fails
                Ok(Promise::new(predecessor_account_id)
                    .transfer(attached_deposit)
                    .then(
                        Self::ext(env::current_account_id())
                            .with_static_gas(FAILURE_CALLBACK_GAS)
                            .on_failure(format!("IAHRegistry::sbt_mint() call failure: {:?}", e)),
                    )
                    .into())
            }
        }
    }
}
