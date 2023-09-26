use anyhow::Ok;
use easy_poll::{PollResult, Results, Status};
use near_sdk::serde_json::json;
use near_units::parse_near;
use test_util::{deploy_contract, get_block_timestamp, registry_default};
use workspaces::{network::Sandbox, Account, AccountId, Contract, Worker};

async fn respond(
    easy_poll_contract: &AccountId,
    responder: &Account,
    poll_id: u64,
) -> anyhow::Result<()> {
    let res = responder
        .call(easy_poll_contract, "respond")
        .args_json(json!({"poll_id": poll_id, "answers": [{"YesNo": true}]}))
        .deposit(parse_near!("1 N"))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success(), "{:?}", res.receipt_failures());
    Ok(())
}

async fn init(worker: &Worker<Sandbox>) -> anyhow::Result<(Contract, Account, Account)> {
    let authority_acc = worker.dev_create_account().await?;
    let flagger = worker.dev_create_account().await?;
    let alice_acc = worker.dev_create_account().await?;
    let bob_acc = worker.dev_create_account().await?;

    // Setup registry contract and issue iah to alice
    let (registry_contract, _) = registry_default(
        &worker,
        authority_acc.id(),
        vec![flagger.id()],
        vec![alice_acc.id()],
    )
    .await?;

    // Setup easy-poll contract
    let easy_poll_contract = deploy_contract(
        &worker,
        "./",
        "new",
        json!({"sbt_registry": registry_contract.id()}),
    )
    .await?;

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
        .args_json(json!({"iah_only": false, "questions": [{"question_type": {"YesNo": false}, "required": true,
            "title": "non-human gated"}], "starts_at": now_ms + 20000, "ends_at": now_ms + 300000,
            "title": "Testing Poll 1", "tags": ["test"], "description": "poll desc", "link": "test.io"}))
        .max_gas()
        .transact()
        .await?
        .json()?;

    // create a poll
    let poll_id_human_gated: u64 = bob.call(easy_poll_contract.id(), "create_poll")
        .args_json(json!({"iah_only": true, "questions": [{"question_type": {"YesNo": false}, "required": true,
            "title": "human gated"}], "starts_at": now_ms + 5000, "ends_at": now_ms + 86400000,
            "title": "Testing Poll 1", "tags": ["test"], "description": "poll desc", "link": "test.io"}))
        .max_gas()
        .transact()
        .await?
        .json()?;

    // fast forward
    worker.fast_forward(100).await?;

    respond(easy_poll_contract.id(), &bob, poll_id_non_human_gated).await?;
    respond(easy_poll_contract.id(), &alice, poll_id_non_human_gated).await?;

    // This vote should not be registered since the poll is human gated and bob is not human
    respond(easy_poll_contract.id(), &bob, poll_id_human_gated).await?;
    respond(easy_poll_contract.id(), &alice, poll_id_human_gated).await?;

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
