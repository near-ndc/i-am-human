use anyhow::Ok;
use easy_poll::{PollResult, Results, Status};
use near_sdk::serde_json::json;
use near_units::parse_near;
use sbt::TokenMetadata;
use test_util::{build_contract, get_block_timestamp};
use workspaces::{network::Sandbox, Account, Contract, Worker};

const IAH_CLASS: u64 = 1;

async fn init(worker: &Worker<Sandbox>) -> anyhow::Result<(Contract, Account, Account)> {
    let authority_acc = worker.dev_create_account().await?;
    let flagger = worker.dev_create_account().await?;
    let iah_issuer = worker.dev_create_account().await?;
    let alice_acc = worker.dev_create_account().await?;
    let bob_acc = worker.dev_create_account().await?;
    // Setup registry contract
    let registry_contract = build_contract(
        &worker,
        "./../registry",
        "new",
        json!({"authority": authority_acc.id(), "authorized_flaggers": vec![flagger.id()], "iah_issuer": iah_issuer.id(), "iah_classes": [1]}),
    ).await?;

    // Setup easy-poll contract
    let easy_poll_contract = build_contract(
        &worker,
        "./",
        "new",
        json!({"sbt_registry": registry_contract.id()}),
    )
    .await?;

    // populate registry with mocked data
    let token_metadata = vec![TokenMetadata {
        class: IAH_CLASS,
        issued_at: Some(0),
        expires_at: None,
        reference: None,
        reference_hash: None,
    }];

    let iah_token_spec = vec![(alice_acc.id(), token_metadata.clone())];

    let res = iah_issuer
        .call(registry_contract.id(), "sbt_mint")
        .args_json(json!({ "token_spec": iah_token_spec }))
        .deposit(parse_near!("1 N"))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success(), "{:?}", res.receipt_failures());

    Ok((easy_poll_contract, alice_acc, bob_acc))
}

#[tokio::test]
async fn flow1() -> anyhow::Result<()> {
    // 1. create non-human gated poll
    // 2. create human gated poll
    // 3. vote for both polls with a human verified account
    // 4. vote for both polls with a non-human account
    // 5. check the responds were recorded correctly

    // import the registry contract from mainnet with data
    let worker = workspaces::sandbox().await?;
    let (easy_poll_contract, alice, bob) = init(&worker).await?;

    let now_ms = get_block_timestamp(&worker).await? / 1_000_000;
    // create a poll
    let poll_id_non_human_gated: u64 = bob.call(easy_poll_contract.id(), "create_poll")
        .args_json(json!({"iah_only": false, "questions": [{"question_type": {"YesNo": false}, "required": true, "title": "non-human gated"}], "starts_at": now_ms + 20000, "ends_at": now_ms + 300000, "title": "Testing Poll 1", "tags": ["test"], "description": "poll desc", "link": "test.io"}))
        .max_gas()
        .transact()
        .await?
        .json()?;

    // create a poll
    let poll_id_human_gated: u64 = bob.call(easy_poll_contract.id(), "create_poll")
        .args_json(json!({"iah_only": true, "questions": [{"question_type": {"YesNo": false}, "required": true, "title": "human gated"}], "starts_at": now_ms + 5000, "ends_at": now_ms + 86400000, "title": "Testing Poll 1", "tags": ["test"], "description": "poll desc", "link": "test.io"}))
        .max_gas()
        .transact()
        .await?
        .json()?;

    // fast forward
    worker.fast_forward(100).await?;

    let res = bob
        .call(easy_poll_contract.id(), "respond")
        .args_json(json!({"poll_id": poll_id_non_human_gated, "answers": [{"YesNo": true}]}))
        .deposit(parse_near!("1 N"))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    let res = alice
        .call(easy_poll_contract.id(), "respond")
        .args_json(json!({"poll_id": poll_id_non_human_gated, "answers": [{"YesNo": true}]}))
        .deposit(parse_near!("1 N"))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success(), "{:?}", res.receipt_failures());

    // This vote should not be registered since the poll is human gated and bob is not human
    let res = bob
        .call(easy_poll_contract.id(), "respond")
        .args_json(json!({"poll_id": poll_id_human_gated, "answers": [{"YesNo": true}]}))
        .deposit(parse_near!("1 N"))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    let res = alice
        .call(easy_poll_contract.id(), "respond")
        .args_json(json!({"poll_id": poll_id_human_gated, "answers": [{"YesNo": true}]}))
        .deposit(parse_near!("1 N"))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    // assert the results are correct
    let res: Option<Results> = bob
        .call(easy_poll_contract.id(), "results")
        .args_json(json!({ "poll_id": poll_id_non_human_gated }))
        .max_gas()
        .transact()
        .await?
        .json()?;

    assert_eq!(
        res.unwrap(),
        Results {
            status: Status::NotStarted,
            participants_num: 2,
            results: vec![PollResult::YesNo((2, 0))]
        }
    );

    let res: Option<Results> = bob
        .call(easy_poll_contract.id(), "results")
        .args_json(json!({ "poll_id": poll_id_human_gated }))
        .max_gas()
        .transact()
        .await?
        .json()?;

    assert_eq!(
        res.unwrap(),
        Results {
            status: Status::NotStarted,
            participants_num: 1,
            results: vec![PollResult::YesNo((1, 0))]
        }
    );

    Ok(())
}
