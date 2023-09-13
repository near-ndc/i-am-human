mod types;
mod utils;
mod workspaces;

use crate::utils::*;
use crate::workspaces::{build_contract, gen_user_account, get_block_timestamp, transfer_near};
use kudos_contract::{utils::*, WrappedCid};
use kudos_contract::{GIVE_KUDOS_COST, LEAVE_COMMENT_COST, UPVOTE_KUDOS_COST};
use near_sdk::serde_json::json;
use near_units::parse_near;

#[tokio::test]
async fn test_required_deposit() -> anyhow::Result<()> {
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

    set_external_db(kudos_contract.id(), &admin_account, &near_social).await?;

    // Register users' accounts
    let test1_account =
        gen_user_account(&worker, &[&"a".repeat(54), ".test.near"].concat()).await?;
    let _ = transfer_near(&worker, test1_account.id(), parse_near!("10 N")).await?;
    let test2_account =
        gen_user_account(&worker, &[&"b".repeat(54), ".test.near"].concat()).await?;
    let _ = transfer_near(&worker, test2_account.id(), parse_near!("10 N")).await?;
    let test3_account =
        gen_user_account(&worker, &[&"c".repeat(54), ".test.near"].concat()).await?;
    let _ = transfer_near(&worker, test3_account.id(), parse_near!("10 N")).await?;

    let now_ms = get_block_timestamp(&worker).await? / 1_000_000;

    // Mint FV SBT for users & verify
    let minted_tokens = mint_fv_sbt(
        &iah_registry_id,
        &admin_account,
        &[test1_account.id(), test2_account.id(), test3_account.id()],
        now_ms,
        now_ms + 86_400_000,
    )
    .await?;
    assert!(verify_is_human(
        &iah_registry_id,
        admin_account.id(),
        &[&test1_account, &test2_account, &test3_account],
        &minted_tokens
    )
    .await
    .is_ok());

    let hashtags = (0..10)
        .map(|n| format!("{}{n}", "a".repeat(31)))
        .collect::<Vec<_>>();
    let kudos_text = "a".repeat(1000);

    // Give kudos
    let Some(balance_1) = storage_balance_of(&near_social_id, kudos_contract.as_account()).await? else {
        anyhow::bail!("Kudos contract wasn't properly initialized at SocialDB!")
    };

    let kudos_id = give_kudos(
        kudos_contract.id(),
        &test1_account,
        test2_account.id(),
        &kudos_text,
        Some(
            WrappedCid::new("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").unwrap(),
        )
        .as_ref(),
        hashtags.iter().map(|s| s.as_str()).collect(),
    )
    .await?;

    let Some(balance_2) = storage_balance_of(&near_social_id, kudos_contract.as_account()).await? else {
        anyhow::bail!("Kudos contract wasn't properly initialized at SocialDB!")
    };

    let consumed =
        (balance_2.total.0 - balance_2.available.0) - (balance_1.total.0 - balance_1.available.0);
    assert!(
        consumed <= GIVE_KUDOS_COST,
        "`give_kudos` call should cost at least {} Ⓝ",
        display_deposit_in_near(consumed)
    );

    // Leave comment (no parent)
    let Some(balance_1) = storage_balance_of(&near_social_id, kudos_contract.as_account()).await? else {
        anyhow::bail!("Kudos contract wasn't properly initialized at SocialDB!")
    };

    let comment_id = leave_comment(
        kudos_contract.id(),
        &test1_account,
        test2_account.id(),
        &kudos_id,
        None,
        &kudos_text,
    )
    .await?;

    let Some(balance_2) = storage_balance_of(&near_social_id, kudos_contract.as_account()).await? else {
        anyhow::bail!("Kudos contract wasn't properly initialized at SocialDB!")
    };

    let consumed =
        (balance_2.total.0 - balance_2.available.0) - (balance_1.total.0 - balance_1.available.0);
    assert!(
        consumed <= LEAVE_COMMENT_COST,
        "`leave_comment` call should cost at least {} Ⓝ",
        display_deposit_in_near(consumed)
    );

    // Leave comment (with parent)
    let Some(balance_1) = storage_balance_of(&near_social_id, kudos_contract.as_account()).await? else {
        anyhow::bail!("Kudos contract wasn't properly initialized at SocialDB!")
    };

    let _ = leave_comment(
        kudos_contract.id(),
        &test1_account,
        test2_account.id(),
        &kudos_id,
        Some(comment_id),
        &kudos_text,
    )
    .await?;

    let Some(balance_2) = storage_balance_of(&near_social_id, kudos_contract.as_account()).await? else {
        anyhow::bail!("Kudos contract wasn't properly initialized at SocialDB!")
    };

    let consumed =
        (balance_2.total.0 - balance_2.available.0) - (balance_1.total.0 - balance_1.available.0);
    println!("{}", display_deposit_in_near(consumed));
    assert!(
        consumed <= LEAVE_COMMENT_COST,
        "`leave_comment` call should cost at least {} Ⓝ",
        display_deposit_in_near(consumed)
    );

    // Upvote kudos
    let Some(balance_1) = storage_balance_of(&near_social_id, kudos_contract.as_account()).await? else {
        anyhow::bail!("Kudos contract wasn't properly initialized at SocialDB!")
    };

    let _ = upvote_kudos(
        kudos_contract.id(),
        &test3_account,
        test2_account.id(),
        &kudos_id,
    )
    .await?;

    let Some(balance_2) = storage_balance_of(&near_social_id, kudos_contract.as_account()).await? else {
        anyhow::bail!("Kudos contract wasn't properly initialized at SocialDB!")
    };

    let consumed =
        (balance_2.total.0 - balance_2.available.0) - (balance_1.total.0 - balance_1.available.0);
    assert!(
        consumed <= UPVOTE_KUDOS_COST,
        "`upvote_kudos` call should cost at least {} Ⓝ",
        display_deposit_in_near(consumed)
    );

    Ok(())
}
