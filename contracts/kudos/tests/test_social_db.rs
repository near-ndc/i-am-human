mod types;
mod utils;
mod workspaces;

use crate::utils::*;
use crate::workspaces::{build_contract, gen_user_account, transfer_near};
use kudos_contract::utils::*;
use kudos_contract::SOCIAL_DB_GRANT_WRITE_PERMISSION_COST;
use near_contract_standards::storage_management::{StorageBalance, StorageBalanceBounds};
use near_sdk::json_types::U128;
use near_sdk::serde_json::{self, json, Value};
use near_sdk::ONE_YOCTO;
use near_units::parse_near;

#[tokio::test]
async fn test_social_db_required_deposit() -> anyhow::Result<()> {
    let worker_mainnet = ::workspaces::mainnet_archival().await?;
    let near_social_id = "social.near".parse()?;
    let worker = ::workspaces::sandbox().await?;

    let admin_account = worker.root_account()?;

    // Setup NEAR Social-DB contract
    let near_social = worker
        .import_contract(&near_social_id, &worker_mainnet)
        .initial_balance(parse_near!("10000000 N"))
        .block_height(94_000_000)
        .transact()
        .await?;
    let _ = near_social
        .call("new")
        .args_json(json!({}))
        .max_gas()
        .transact()
        .await?
        .into_result()?;
    let _ = near_social
        .call("set_status")
        .args_json(json!({"status": "Live"}))
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    // Initialize NDC i-am-human registry contract
    let iah_registry_id = "registry.i-am-human.near".parse()?;
    let iah_registry = worker
        .import_contract(&iah_registry_id, &worker_mainnet)
        .initial_balance(parse_near!("10000000 N"))
        .block_height(95_309_837)
        .transact()
        .await?;
    let _ = iah_registry
        .call("new")
        .args_json(json!({
          "authority": admin_account.id(),
          "iah_issuer": admin_account.id(),
          "iah_classes": [1]
        }))
        .max_gas()
        .transact()
        .await?
        .into_result()?;
    let _ = admin_account
        .call(&iah_registry_id, "admin_add_sbt_issuer")
        .args_json(json!({
          "issuer": admin_account.id()
        }))
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    // Setup NDC Kudos Contract
    let kudos_contract = build_contract(
        &worker,
        "./",
        "init",
        json!({ "iah_registry": iah_registry_id, "owner_id": admin_account.id() }),
    )
    .await?;

    let social_db_balance_bounds: StorageBalanceBounds = near_social
        .view("storage_balance_bounds")
        .args_json(json!({}))
        .await?
        .json()?;

    let contract_with_max_name =
        gen_user_account(&worker, &[&"y".repeat(54), ".test.near"].concat()).await?;
    let initial_json = serde_json::from_str::<Value>(&format!(
        r#"{{
          "{}": {{
            "kudos": {{}},
            "hashtags": {{}}
          }}
        }}"#,
        contract_with_max_name.id()
    ))?;
    let _ = contract_with_max_name
        .call(&near_social_id, "set")
        .args_json(json!({ "data": initial_json }))
        .deposit(social_db_balance_bounds.min.0)
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    let StorageBalance {
        total: U128(total_before),
        available: U128(available_before),
    } = storage_balance_of(&near_social_id, &contract_with_max_name)
        .await?
        .unwrap();

    let grant_permissions_for =
        gen_user_account(&worker, &[&"z".repeat(54), ".test.near"].concat()).await?;

    let _ = contract_with_max_name
        .call(&near_social_id, "grant_write_permission")
        .args_json(json!({
          "predecessor_id": grant_permissions_for.id(),
          "keys": vec![format!("{}", contract_with_max_name.id())]
        }))
        .deposit(ONE_YOCTO)
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    let StorageBalance {
        total: U128(total_after),
        available: U128(available_after),
    } = storage_balance_of(&near_social_id, &contract_with_max_name)
        .await?
        .unwrap();

    assert_eq!(
        total_before + ONE_YOCTO,
        total_after,
        "Initital total deposit before & after `SocialDB::grant_write_permission` call should diff only by 1 yocto!"
    );

    let required_deposit_for_grant_write_permission = available_before - available_after;
    assert!(
        required_deposit_for_grant_write_permission <= SOCIAL_DB_GRANT_WRITE_PERMISSION_COST,
        "Pre-computed deposit requirements for `SocialDB::grant_write_permission` call is less than required ({} < {})!",
        display_deposit_in_near(SOCIAL_DB_GRANT_WRITE_PERMISSION_COST),
        display_deposit_in_near(required_deposit_for_grant_write_permission)
    );

    set_external_db(kudos_contract.id(), &admin_account, &near_social).await?;

    let StorageBalance {
        total: U128(kudos_contract_total),
        available: U128(kudos_contract_available),
    } = storage_balance_of(&near_social_id, kudos_contract.as_account())
        .await?
        .unwrap();

    assert_eq!(
        kudos_contract_total, total_after,
        "Kudos contract initialized at SocialDB initially deposits incorrect amount!"
    );
    assert!(
        kudos_contract_available >= available_after,
        "Kudos contract initialized at SocialDB uses storage more than expected (available: {}, expected: {})!",
        display_deposit_in_near(kudos_contract_available),
        display_deposit_in_near(available_after),
    );

    let fake_iah_registry =
        gen_user_account(&worker, &[&"x".repeat(54), ".test.near"].concat()).await?;
    let _ = transfer_near(&worker, fake_iah_registry.id(), parse_near!("5 N")).await?;

    update_iah_registry(kudos_contract.id(), &admin_account, fake_iah_registry.id()).await?;

    let StorageBalance {
        total: U128(kudos_contract_total_after_iah_update),
        available: U128(kudos_contract_available_after_iah_update),
    } = storage_balance_of(&near_social_id, kudos_contract.as_account())
        .await?
        .unwrap();

    assert!(
        kudos_contract_total <= kudos_contract_total_after_iah_update,
        "Kudos contract initialized at SocialDB total storage deposit can't be less than before call `update_iah_registry` method!"
    );
    assert!(
        kudos_contract_available <= kudos_contract_available_after_iah_update,
        "Kudos contract initialized at SocialDB available storage deposit can't be less than before call `update_iah_registry` method ({} < {})!",
        display_deposit_in_near(kudos_contract_available_after_iah_update),
        display_deposit_in_near(kudos_contract_available),
    );

    // Check that permission were granted
    let updated_data = serde_json::from_str::<Value>(&format!(
        r#"{{
          "{}": {{
            "test": "test_value"
          }}
        }}"#,
        kudos_contract.id()
    ))?;
    let _ = fake_iah_registry
        .call(&near_social_id, "set")
        .args_json(json!({ "data": updated_data }))
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    Ok(())
}
