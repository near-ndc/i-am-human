use anyhow::Ok;
use near_sdk::serde_json::json;
use near_units::parse_near;
use near_workspaces::{network::Sandbox, Account, AccountId, Contract, Worker};
use registry::storage::AccountFlag;
use sbt::{ClassSet, TokenMetadata};

const MAINNET_REGISTRY_ID: &str = "registry.i-am-human.near";
const BLOCK_HEIGHT: u64 = 92042705;
const IAH_CLASS: u64 = 1;
const OG_CLASS: u64 = 2;

async fn assert_data_consistency(
    registry: &Contract,
    iah_issuer: &Account,
    og_issuer: &Account,
    alice: &Account,
    bob: &Account,
) -> anyhow::Result<()> {
    // run queries before the migration
    let og_supply: u64 = registry
        .call("sbt_supply")
        .args_json(json!({"issuer": og_issuer.id()}))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(og_supply, 2);

    let iah_supply: u64 = registry
        .call("sbt_supply")
        .args_json(json!({"issuer": iah_issuer.id()}))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(iah_supply, 4);

    let og_supply_by_class: u64 = registry
        .call("sbt_supply_by_class")
        .args_json(json!({"issuer": og_issuer.id(), "class": OG_CLASS}))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(og_supply_by_class, 2);

    let iah_supply_by_class: u64 = registry
        .call("sbt_supply_by_class")
        .args_json(json!({"issuer": iah_issuer.id(), "class": IAH_CLASS}))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(iah_supply_by_class, 4);

    let alice_iah_supply: u64 = registry
        .call("sbt_supply_by_owner")
        .args_json(json!({"account": alice.id(), "issuer": iah_issuer.id(), "class": null}))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(alice_iah_supply, 1);

    let alice_og_supply: u64 = registry
        .call("sbt_supply_by_owner")
        .args_json(json!({"account": alice.id(), "issuer": og_issuer.id(), "class": null}))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(alice_og_supply, 1);

    let bob_iah_supply: u64 = registry
        .call("sbt_supply_by_owner")
        .args_json(json!({"account": bob.id(), "issuer": iah_issuer.id(), "class": null}))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(bob_iah_supply, 1);

    let bob_og_supply: u64 = registry
        .call("sbt_supply_by_owner")
        .args_json(json!({"account": bob.id(), "issuer": og_issuer.id(), "class": null}))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(bob_og_supply, 0);

    let iah_class_set: ClassSet = registry
        .call("iah_class_set")
        .args_json(json!({}))
        .max_gas()
        .transact()
        .await?
        .json()?;

    assert_eq!(iah_class_set[0].0.to_string(), iah_issuer.id().to_string());
    assert_eq!(iah_class_set[0].1[0], 1);

    Ok(())
}

async fn init(
    worker: &Worker<Sandbox>,
) -> anyhow::Result<(Account, Account, Account, Contract, Account, Account)> {
    // import the contract from mainnet
    let worker_mainnet = near_workspaces::mainnet().await?;
    let contract_id: AccountId = MAINNET_REGISTRY_ID.parse()?;
    let registry_contract = worker
        .import_contract(&contract_id, &worker_mainnet)
        .initial_balance(parse_near!("10000000 N"))
        .transact()
        .await?;

    let registry_mainnet = registry_contract.as_account();
    let authority_acc = worker.dev_create_account().await?;
    let flagger = worker.dev_create_account().await?;
    let iah_issuer = worker.dev_create_account().await?;
    let og_issuer = worker.dev_create_account().await?;
    let alice_acc = worker.dev_create_account().await?;
    let bob_acc = worker.dev_create_account().await?;
    let john_acc = worker.dev_create_account().await?;
    let elon_acc = worker.dev_create_account().await?;

    // init the contract
    let res = registry_contract
        .call("new")
        .args_json(json!({"authority": authority_acc.id(),
                          "authorized_flaggers": vec![flagger.id()],
                          "iah_issuer": iah_issuer.id(), "iah_classes": [1]}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success(), "{:?}", res.receipt_failures());

    // add iah_issuer
    let res = authority_acc
        .call(registry_mainnet.id(), "admin_add_sbt_issuer")
        .args_json(json!({"issuer": iah_issuer.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success(), "{:?}", res.receipt_failures());

    // add og_issuer
    let res = authority_acc
        .call(registry_mainnet.id(), "admin_add_sbt_issuer")
        .args_json(json!({"issuer": og_issuer.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success(), "{:?}", res.receipt_failures());

    // populate registry with mocked data
    let mut token_metadata = vec![TokenMetadata {
        class: 1,
        issued_at: Some(0),
        expires_at: None,
        reference: None,
        reference_hash: None,
    }];

    let iah_token_spec = vec![
        (alice_acc.id(), token_metadata.clone()),
        (bob_acc.id(), token_metadata.clone()),
        (john_acc.id(), token_metadata.clone()),
        (elon_acc.id(), token_metadata.clone()),
    ];

    let res = iah_issuer
        .call(registry_mainnet.id(), "sbt_mint")
        .args_json(json!({ "token_spec": iah_token_spec }))
        .deposit(parse_near!("1 N"))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success(), "{:?}", res.receipt_failures());

    token_metadata[0].class = 2;
    let og_token_spec = vec![
        (alice_acc.id(), token_metadata.clone()),
        (elon_acc.id(), token_metadata),
    ];

    let res = og_issuer
        .call(registry_mainnet.id(), "sbt_mint")
        .args_json(json!({ "token_spec": og_token_spec }))
        .deposit(parse_near!("1 N"))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success(), "{:?}", res.receipt_failures());

    Ok((
        registry_mainnet.clone(),
        iah_issuer,
        og_issuer,
        registry_contract,
        alice_acc,
        bob_acc,
    ))
}

#[ignore = "this test is not valid after the migration"]
#[tokio::test]
async fn migration_mainnet() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let (registry, iah_issuer, og_issuer, old_registry_contract, alice, bob) =
        init(&worker).await?;

    // run queries before the migration
    assert_data_consistency(
        &old_registry_contract,
        &iah_issuer,
        &og_issuer,
        &alice,
        &bob,
    )
    .await?;

    // deploy the new contract
    let new_registry_contract = registry
        .deploy(include_bytes!("../../deployed/registry.wasm"))
        .await?
        .into_result()?;

    // call the migrate method
    let res = new_registry_contract
        .call("migrate")
        .args_json(json!({"authorized_flaggers": [alice.id()]}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success(), "{:?}", res.receipt_failures());

    // run queries after the migration
    assert_data_consistency(
        &new_registry_contract,
        &iah_issuer,
        &og_issuer,
        &alice,
        &bob,
    )
    .await?;

    let res = new_registry_contract
        .call("account_flagged")
        .args_json(json!({"account": "bob.near"}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());
    let res: Option<AccountFlag> = res.json()?;
    assert!(res.is_none());

    let res = alice
        .call(new_registry_contract.id(), "admin_flag_accounts")
        .args_json(
            json!({"flag": AccountFlag::Blacklisted,"accounts": vec!["bob.near"], "memo": "test"}),
        )
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success(), "{:?}", res.receipt_failures());

    let res = new_registry_contract
        .call("account_flagged")
        .args_json(json!({"account": "bob.near"}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());
    let res: Option<AccountFlag> = res.json()?;
    assert_eq!(res.unwrap(), AccountFlag::Blacklisted);

    Ok(())
}

#[ignore = "this test is not valid after the migration"]
// handler error: [State of contract registry.i-am-human.near is too large to be viewed]
// The current running registry contract is too large to be viewed.
// This test cannot be perfomed on real data anymore
#[tokio::test]
async fn migration_mainnet_real_data() -> anyhow::Result<()> {
    // import the registry contract from mainnet with data
    let worker = near_workspaces::sandbox().await?;
    let worker_mainnet = near_workspaces::mainnet_archival().await?;
    let contract_id: AccountId = MAINNET_REGISTRY_ID.parse()?;
    let old_registry_contract = worker
        .import_contract(&contract_id, &worker_mainnet)
        .initial_balance(parse_near!("10000000 N"))
        .block_height(BLOCK_HEIGHT)
        .with_data()
        .transact()
        .await?;

    // run queries before the migration
    let supply: u64 = old_registry_contract
        .call("sbt_supply")
        .args_json(json!({"issuer": "gooddollar-v1.i-am-human.near"}))
        .max_gas()
        .transact()
        .await?
        .json()?;

    // deploy the new contract
    let new_registry_mainnet = old_registry_contract
        .as_account()
        .deploy(include_bytes!("../../deployed/registry.wasm"))
        .await?
        .into_result()?;

    // call the migrate method
    let res = new_registry_mainnet
        .call("migrate")
        .args_json(json!({"authorized_flaggers": ["alice.near"]}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success(), "{:?}", res.receipt_failures());

    // run queries after the migration
    let res: u64 = new_registry_mainnet
        .call("sbt_supply")
        .args_json(json!({"issuer": "gooddollar-v1.i-am-human.near"}))
        .max_gas()
        .transact()
        .await?
        .json()?;

    assert_eq!(supply, res);

    Ok(())
}
