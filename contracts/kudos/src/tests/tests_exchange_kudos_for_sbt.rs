use crate::consts::{EXCHANGE_KUDOS_COST, EXCHANGE_KUDOS_STORAGE};
use crate::tests::utils::{build_default_context, promise_or_value_result_into_result, MAX_GAS};
use crate::utils::{build_kudos_kind_path, build_kudos_upvotes_path};
use crate::{Contract, IncrementalUniqueId, KudosId, PROOF_OF_KUDOS_SBT_MINT_COST};
use near_sdk::borsh::BorshSerialize;
use near_sdk::serde_json::json;
use near_sdk::test_utils::accounts;
use near_sdk::{
    env, testing_env, AccountId, Gas, PromiseError, PromiseResult, RuntimeFeesConfig, VMConfig,
};
use std::collections::HashMap;

#[test]
fn test_required_storage_to_exchange_kudos() {
    testing_env!(build_default_context(accounts(0), None, Some(Gas::ONE_TERA)).build());

    let mut kudos_contract = Contract::init(
        Some(accounts(0)),
        AccountId::new_unchecked("iah_registry.near".to_owned()),
    );

    let initial_storage = env::storage_usage();
    kudos_contract
        .exchanged_kudos
        .insert(IncrementalUniqueId::default().next().into());
    assert_eq!(
        env::storage_usage() - initial_storage,
        EXCHANGE_KUDOS_STORAGE
    );
}

#[test]
fn test_required_deposit_to_exchange_kudos() -> anyhow::Result<()> {
    let contract_id = AccountId::new_unchecked("kudos.near".to_owned());
    testing_env!(
        build_default_context(contract_id.clone(), None, Some(MAX_GAS),)
            .attached_deposit(EXCHANGE_KUDOS_COST)
            .prepaid_gas(MAX_GAS)
            .build(),
        VMConfig::test(),
        RuntimeFeesConfig::test(),
        HashMap::default(),
        vec![PromiseResult::Successful(vec![1u64].try_to_vec().unwrap())],
    );

    let initial_balance = env::account_balance();
    let mut kudos_contract = Contract::init(
        Some(contract_id.clone()),
        AccountId::new_unchecked("iah_registry.near".to_owned()),
    );

    let kudos_id = KudosId::from(IncrementalUniqueId::default().next());
    let receiver_id = accounts(0);
    let sender_id = accounts(1);
    let kudos_upvotes_path = build_kudos_upvotes_path(&contract_id, &receiver_id, &kudos_id);
    let kudos_kind_path = build_kudos_kind_path(&contract_id, &receiver_id, &kudos_id);
    kudos_contract.on_kudos_upvotes_acquired(
        sender_id.clone(),
        EXCHANGE_KUDOS_COST.into(),
        kudos_id.clone(),
        kudos_upvotes_path.clone(),
        kudos_kind_path.clone(),
        Ok(json!({
            "kudos.near": {
              "kudos": {
                "alice": {
                  "1": {
                    "kind": "k",
                    "upvotes": {
                      "charlie": "",
                      "danny": "",
                      "eugene": ""
                    }
                  }
                }
              }
            }
        })),
    );
    // There is no way to verify if callback failed or not, because it never panics and
    // calls another failure callback in case of failure. So we verify balance change,
    // if we get full refund then it's an error, otherwise we attach `PROOF_OF_KUDOS_SBT_MINT_COST`
    // to next XCC
    let used_deposit = initial_balance - env::account_balance();
    assert_eq!(used_deposit, PROOF_OF_KUDOS_SBT_MINT_COST);

    let initial_balance = env::account_balance();
    kudos_contract.on_kudos_upvotes_acquired(
        sender_id.clone(),
        EXCHANGE_KUDOS_COST.into(),
        kudos_id.clone(),
        kudos_upvotes_path.clone(),
        kudos_kind_path.clone(),
        Ok(json!({
            "kudos.near": {
              "kudos": {
                "alice": {
                  "1": {
                    "upvotes": {}
                  }
                }
              }
            }
        })),
    );
    // Not enough upvotes, full attached deposit returned
    let transferred_deposit = initial_balance - env::account_balance();
    assert_eq!(transferred_deposit, EXCHANGE_KUDOS_COST);

    let initial_balance = env::account_balance();
    kudos_contract.on_kudos_upvotes_acquired(
        sender_id,
        EXCHANGE_KUDOS_COST.into(),
        kudos_id,
        kudos_upvotes_path,
        kudos_kind_path,
        Ok(json!({
            "kudos.near": {
              "kudos": {
                "alice": {
                  "1": {
                    "kind": "d",
                    "upvotes": {
                      "charlie": "",
                      "danny": "",
                      "eugene": ""
                    }
                  }
                }
              }
            }
        })),
    );
    // Ding kind couldn't be exchanged, full attached deposit returned
    let transferred_deposit = initial_balance - env::account_balance();
    assert_eq!(transferred_deposit, EXCHANGE_KUDOS_COST);

    Ok(())
}

#[test]
fn test_on_pok_sbt_mint() {
    let contract_id = AccountId::new_unchecked("kudos.near".to_owned());
    let context = build_default_context(contract_id.clone(), None, Some(MAX_GAS))
        .attached_deposit(EXCHANGE_KUDOS_COST)
        .build();

    let mut kudos_contract = Contract::init(
        Some(contract_id),
        AccountId::new_unchecked("iah_registry.near".to_owned()),
    );

    let sender_id = accounts(0);
    let kudos_id = KudosId::from(IncrementalUniqueId::default().next());

    struct TestCase<'a> {
        name: &'a str,
        input: Result<Vec<u64>, PromiseError>,
        output: Result<String, String>,
    }

    let test_cases = [
        TestCase {
            name: "SBT mint successful",
            input: Ok(vec![1u64]),
            output: Ok("[1]".to_owned()),
        },
        TestCase {
            name: "SBT mint failure",
            input: Ok(vec![]),
            output: Err("IAHRegistry::sbt_mint() responses with an empty tokens array".to_owned()),
        },
        TestCase {
            name: "Promise error",
            input: Err(near_sdk::PromiseError::Failed),
            output: Ok("Promise".to_owned()),
        },
    ];

    for test_case in test_cases {
        testing_env!(context.clone());

        assert_eq!(
            promise_or_value_result_into_result(kudos_contract.on_pok_sbt_mint(
                sender_id.clone(),
                EXCHANGE_KUDOS_COST.into(),
                kudos_id.clone(),
                test_case.input
            )),
            test_case.output,
            "Test case `{} failure`",
            test_case.name
        );
    }
}
