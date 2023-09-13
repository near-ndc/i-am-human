mod types;
mod utils;
mod workspaces;

use crate::types::*;
use crate::utils::*;
use crate::workspaces::{build_contract, gen_user_account, get_block_timestamp, transfer_near};
use kudos_contract::WrappedCid;
use kudos_contract::{utils::*, CommentId};
use kudos_contract::{Commentary, PROOF_OF_KUDOS_SBT_CLASS_ID};
use near_sdk::serde_json::{self, json, Value};
use near_sdk::AccountId;
use near_units::parse_near;
use std::collections::{BTreeMap, HashMap};

#[tokio::test]
async fn test_give_kudos() -> anyhow::Result<()> {
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
        json!({ "iah_registry": iah_registry_id }),
    )
    .await?;

    set_external_db(
        kudos_contract.id(),
        kudos_contract.as_account(),
        &near_social,
    )
    .await?;

    // Register users' accounts
    let user1_account = gen_user_account(&worker, "user1.test.near").await?;
    let _ = transfer_near(&worker, user1_account.id(), parse_near!("50 N")).await?;

    let user2_account = gen_user_account(&worker, "user2.test.near").await?;
    let _ = transfer_near(&worker, user2_account.id(), parse_near!("50 N")).await?;

    let user3_account = gen_user_account(&worker, "user3.test.near").await?;
    let _ = transfer_near(&worker, user3_account.id(), parse_near!("50 N")).await?;

    let now_ms = get_block_timestamp(&worker).await? / 1_000_000;

    // Mint FV SBT for users & verify
    let minted_tokens: Vec<u64> = mint_fv_sbt(
        &iah_registry_id,
        &admin_account,
        &[user1_account.id(), user2_account.id(), user3_account.id()],
        now_ms,
        now_ms + 86_400_000,
    )
    .await?;
    assert!(verify_is_human(
        &iah_registry_id,
        admin_account.id(),
        &[&user1_account, &user2_account, &user3_account],
        &minted_tokens
    )
    .await
    .is_ok());

    // User1 gives kudos to User2
    let hashtags = (0..3).map(|n| format!("ht_{n}")).collect::<Vec<_>>();
    let kudos_message = "test\",\n\"a\":{\"b\":\"test2\"},\"c\":\"msg";
    let kudos_id = give_kudos(
        kudos_contract.id(),
        &user1_account,
        user2_account.id(),
        kudos_message,
        Some(
            &WrappedCid::new("bafybeihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku")
                .unwrap(),
        ),
        hashtags.iter().map(|s| s.as_str()).collect(),
    )
    .await?;

    let get_kudos_by_id_req = build_get_kudos_by_id_request(
        &AccountId::new_unchecked(kudos_contract.id().to_string()),
        &AccountId::new_unchecked(user2_account.id().to_string()),
        &kudos_id,
    );

    let hashtags_req = format!("{}/hashtags/**", kudos_contract.id());

    // Verify kudos on NEAR Social-DB contract
    let mut kudos_data: near_sdk::serde_json::Value = user2_account
        .view(&near_social_id, "get")
        .args_json(json!({ "keys": [get_kudos_by_id_req, hashtags_req] }))
        .await?
        .json()?;
    // remove `created_at` nested key to be able compare with static stringified json and verify that removed key were exist
    assert!(remove_key_from_json(
        &mut kudos_data,
        &get_kudos_by_id_req.replace('*', "created_at")
    )
    .is_some());
    let extracted_hashtags = remove_key_from_json(
        &mut kudos_data,
        &format!("{}/hashtags", kudos_contract.id()),
    )
    .and_then(|val| serde_json::from_value::<BTreeMap<String, Value>>(val).ok())
    .map(|map| map.keys().cloned().collect::<Vec<_>>());
    assert_eq!(extracted_hashtags, Some(hashtags.clone()));

    let escaped_kudos_message = kudos_message.escape_default().to_string();
    assert_eq!(
        kudos_data.to_string(),
        format!(
            r#"{{"{}":{{"kudos":{{"{}":{{"{kudos_id}":{{"icon":"bafybeihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku","kind":"k","message":"{escaped_kudos_message}","sender_id":"{}","tags":"{}"}}}}}}}}}}"#,
            kudos_contract.id(),
            user2_account.id(),
            user1_account.id(),
            serde_json::to_string(&hashtags)
                .unwrap()
                .escape_default(),
        )
    );

    // User3 upvotes kudos given to User2 by User1
    let _ = upvote_kudos(
        kudos_contract.id(),
        &user3_account,
        user2_account.id(),
        &kudos_id,
    )
    .await?;

    // Verify upvoted kudos on NEAR Social-DB contract
    let mut kudos_data: near_sdk::serde_json::Value = user2_account
        .view(&near_social_id, "get")
        .args_json(json!({
            "keys": [get_kudos_by_id_req.replace('*', "upvotes/**")]
        }))
        .await?
        .json()?;

    // remove `/upvotes` nested key and check for it's value, which should contain User3 who upvoted kudos
    let upvotes_json = remove_key_from_json(
        &mut kudos_data,
        &get_kudos_by_id_req.replace('*', "upvotes"),
    )
    .unwrap()
    .to_string();
    assert_eq!(upvotes_json, format!(r#"{{"{}":""}}"#, user3_account.id()));

    // User3 leaves a comment to kudos given to User2 by User1
    let comment1_id = leave_comment(
        kudos_contract.id(),
        &user3_account,
        user2_account.id(),
        &kudos_id,
        None,
        "amazing",
    )
    .await?;

    // User2 leaves a reply to a comment from User3
    let comment2_id = leave_comment(
        kudos_contract.id(),
        &user2_account,
        user2_account.id(),
        &kudos_id,
        Some(comment1_id.clone()),
        "wow",
    )
    .await?;

    // User3 leaves a reply to a comment from User2
    let comment3_id = leave_comment(
        kudos_contract.id(),
        &user3_account,
        user2_account.id(),
        &kudos_id,
        Some(comment2_id.clone()),
        "you are the best",
    )
    .await?;

    // User3 fails to leave a reply to an invalid comment id
    let err = leave_comment(
        kudos_contract.id(),
        &user3_account,
        user2_account.id(),
        &kudos_id,
        Some(CommentId::new_unchecked(123456789)),
        "failure",
    )
    .await
    .unwrap_err();
    assert_eq!(
        &err.to_string(),
        r#"Leave comment failure: Action #0: ExecutionError("Smart contract panicked: Unable to verify parent commentary id")"#
    );

    // Verify comment left for kudos on NEAR Social-DB contract
    let mut kudos_data: near_sdk::serde_json::Value = user2_account
        .view(&near_social_id, "get")
        .args_json(json!({
            "keys": [get_kudos_by_id_req.replace('*', "comments/**")]
        }))
        .await?
        .json()?;

    // remove `/comments` nested key and check for it's value, which should contain User3 who left a comment and a message for kudos
    let comments_json = remove_key_from_json(
        &mut kudos_data,
        &get_kudos_by_id_req.replace('*', "comments"),
    )
    .unwrap();
    let comments =
        serde_json::from_value::<HashMap<CommentId, CommentaryOwned>>(comments_json).unwrap();

    // verify first comment
    let comment = Commentary::from(comments.get(&comment1_id).unwrap());
    assert_eq!(
        comment.sender_id.as_str(),
        user3_account
            .id()
            .parse::<near_sdk::AccountId>()
            .unwrap()
            .as_str()
    );
    assert_eq!(comment.message, "amazing");

    // verify a reply comment
    let comment = Commentary::from(comments.get(&comment2_id).unwrap());
    assert_eq!(
        comment.sender_id.as_str(),
        user2_account
            .id()
            .parse::<near_sdk::AccountId>()
            .unwrap()
            .as_str()
    );
    assert_eq!(comment.message, "wow");
    assert_eq!(comment.parent_comment_id, Some(&comment1_id));

    // verify a reply comment
    let comment = Commentary::from(comments.get(&comment3_id).unwrap());
    assert_eq!(
        comment.sender_id.as_str(),
        user3_account
            .id()
            .parse::<near_sdk::AccountId>()
            .unwrap()
            .as_str()
    );
    assert_eq!(comment.message, "you are the best");
    assert_eq!(comment.parent_comment_id, Some(&comment2_id));

    Ok(())
}

#[tokio::test]
async fn test_mint_proof_of_kudos_sbt() -> anyhow::Result<()> {
    let worker_mainnet = ::workspaces::mainnet_archival().await?;
    let near_social_id = "social.near".parse()?;
    let worker = ::workspaces::sandbox().await?;

    let admin_account = worker.root_account()?;
    let iah_registry_id = "registry.i-am-human.near".parse()?;

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

    // Setup NDC Kudos Contract
    let kudos_contract = build_contract(
        &worker,
        "./",
        "init",
        json!({ "iah_registry": iah_registry_id }),
    )
    .await?;

    set_external_db(
        kudos_contract.id(),
        kudos_contract.as_account(),
        &near_social,
    )
    .await?;

    // Initialize NDC i-am-human registry contract
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
    // Set Kudos contract as an SBT issuer
    let _ = admin_account
        .call(&iah_registry_id, "admin_add_sbt_issuer")
        .args_json(json!({
          "issuer": kudos_contract.id()
        }))
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    // Register users' accounts
    let user1_account = gen_user_account(&worker, "user1.test.near").await?;
    let _ = transfer_near(&worker, user1_account.id(), parse_near!("10 N")).await?;

    let user2_account = gen_user_account(&worker, "user2.test.near").await?;
    let _ = transfer_near(&worker, user2_account.id(), parse_near!("10 N")).await?;
    let user3_account = gen_user_account(&worker, "user3.test.near").await?;
    let _ = transfer_near(&worker, user3_account.id(), parse_near!("10 N")).await?;

    let user4_account = gen_user_account(&worker, "user4.test.near").await?;
    let _ = transfer_near(&worker, user4_account.id(), parse_near!("10 N")).await?;

    let user5_account = gen_user_account(&worker, "user5.test.near").await?;
    let _ = transfer_near(&worker, user5_account.id(), parse_near!("10 N")).await?;

    let now_ms = get_block_timestamp(&worker).await? / 1_000_000;

    // Mint FV SBT for users
    let _ = mint_fv_sbt(
        &iah_registry_id,
        &admin_account,
        &[user1_account.id(),
            user2_account.id(),
            user3_account.id(),
            user4_account.id(),
            user5_account.id()],
        now_ms,
        now_ms + 86_400_000,
    )
    .await?;

    // User2 gives kudos to User1
    let kudos_id = give_kudos(
        kudos_contract.id(),
        &user2_account,
        user1_account.id(),
        "blablabla sdfsdfsd\nfsdfsdfs 🚀\n😎✨",
        None,
        vec!["ht-a", "ht_b"],
    )
    .await?;

    // User3 upvotes kudos for User1
    let _ = upvote_kudos(
        kudos_contract.id(),
        &user3_account,
        user1_account.id(),
        &kudos_id,
    )
    .await?;

    // User4 upvotes kudos for User1
    let _ = upvote_kudos(
        kudos_contract.id(),
        &user4_account,
        user1_account.id(),
        &kudos_id,
    )
    .await?;

    // User5 upvotes kudos for User1
    let _ = upvote_kudos(
        kudos_contract.id(),
        &user5_account,
        user1_account.id(),
        &kudos_id,
    )
    .await?;

    // User1 exchanges his Kudos for ProofOfKudos SBT
    let tokens_ids = exchange_kudos_for_sbt(kudos_contract.id(), &user1_account, &kudos_id).await?;
    assert_eq!(tokens_ids, vec![PROOF_OF_KUDOS_SBT_CLASS_ID]);

    verify_kudos_sbt_tokens_by_owner(
        &iah_registry_id,
        kudos_contract.id(),
        &user1_account,
        &tokens_ids,
    )
    .await?;

    Ok(())
}

#[tokio::test]
async fn test_mass_give_kudos() -> anyhow::Result<()> {
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
        json!({ "iah_registry": iah_registry_id }),
    )
    .await?;

    set_external_db(
        kudos_contract.id(),
        kudos_contract.as_account(),
        &near_social,
    )
    .await?;

    // Register users' accounts
    let number_of_users: usize = 5;
    let mut users_accounts = Vec::with_capacity(number_of_users);

    for i in 0..number_of_users {
        let user_account = gen_user_account(&worker, &format!("user{}.test.near", &i)).await?;
        let _ = transfer_near(&worker, user_account.id(), parse_near!("5 N")).await?;
        users_accounts.push(user_account);
    }

    let now_ms = get_block_timestamp(&worker).await? / 1_000_000;

    // Mint FV SBT for users & verify
    let _ = mint_fv_sbt(
        &iah_registry_id,
        &admin_account,
        &users_accounts
            .iter()
            .map(|user| user.id())
            .collect::<Vec<_>>(),
        now_ms,
        now_ms + 86_400_000,
    )
    .await?;

    let mut kudos = vec![];

    for user_account in &users_accounts[1..] {
        // UserX gives kudos to User1
        let hashtags = (0..3).map(|n| format!("ht{n}")).collect::<Vec<_>>();
        let kudos_message = "amazing message".repeat(32);
        let kudos_id = give_kudos(
            kudos_contract.id(),
            user_account,
            users_accounts.first().unwrap().id(),
            &kudos_message,
            None,
            hashtags.iter().map(|s| s.as_str()).collect(),
        )
        .await?;
        kudos.push(kudos_id);
    }

    for (user_account, kudos_id) in users_accounts[1..].iter().rev().zip(&kudos) {
        println!("{} upvotes kudos {}", user_account.id(), kudos_id);
        // UserX upvotes kudos of User1
        let _ = upvote_kudos(
            kudos_contract.id(),
            user_account,
            users_accounts.first().unwrap().id(),
            kudos_id,
        )
        .await?;
    }

    Ok(())
}
