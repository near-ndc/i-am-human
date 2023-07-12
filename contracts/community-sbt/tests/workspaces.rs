use anyhow::Ok;
use near_units::parse_near;
use sbt::TokenMetadata;
use serde_json::json;
use workspaces::{network::Sandbox, Account, AccountId, Contract, Worker};

const MAINNET_REGISTRY_ID: &str = "registry-v1.gwg-testing.near";
const MAINNET_COMMUNITY_SBT_ID: &str = "community-testing.i-am-human.near";

async fn init(worker: &Worker<Sandbox>) -> anyhow::Result<(Account, Account, Contract, Account)> {
    // import the registry contract from mainnet
    let worker_mainnet = workspaces::mainnet().await?;
    let contract_id: AccountId = MAINNET_REGISTRY_ID.parse()?;
    let registry_contract = worker
        .import_contract(&contract_id, &worker_mainnet)
        .initial_balance(parse_near!("10000000 N"))
        .transact()
        .await?;

    // import the community-sbt contract from mainnet
    let contract_id: AccountId = MAINNET_COMMUNITY_SBT_ID.parse()?;
    let community_contract = worker
        .import_contract(&contract_id, &worker_mainnet)
        .initial_balance(parse_near!("10000000 N"))
        .transact()
        .await?;

    let registry_mainnet = registry_contract.as_account();
    let community_mainnet = community_contract.as_account();
    let authority_acc = worker.dev_create_account().await?;
    let iah_issuer = worker.dev_create_account().await?;
    let alice_acc = worker.dev_create_account().await?;
    let bob_acc = worker.dev_create_account().await?;

    // init the registry
    let res = registry_contract
        .call("new")
        .args_json(json!({"authority": authority_acc.id(), "iah_issuer": iah_issuer.id(), "iah_classes": [1] }))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    // init the community-sbt
    let res = community_contract
        .call("new")
        .args_json(
            json!({"registry": registry_mainnet.id(), "admin": authority_acc.id(), "metadata": {
            "spec": "sbt-1.0.0",
            "name": "Community SBT",
            "symbol": "CoSBT"
          }, "ttl": 2147483647 }),
        )
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    // add iah_issuer
    let res = authority_acc
        .call(registry_mainnet.id(), "admin_add_sbt_issuer")
        .args_json(json!({"issuer": iah_issuer.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    // add community_issuer
    let res = authority_acc
        .call(registry_mainnet.id(), "admin_add_sbt_issuer")
        .args_json(json!({"issuer": community_mainnet.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    // authorize authority to mint tokens
    let res = authority_acc
        .call(community_mainnet.id(), "authorize")
        .args_json(json!({"class": 1, "minter": authority_acc.id(), "memo": "test"}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    // mint mocked community tokens
    let token_metadata = TokenMetadata {
        class: 1,
        issued_at: Some(0),
        expires_at: None,
        reference: None,
        reference_hash: None,
    };

    let res = authority_acc
        .call(community_mainnet.id(), "sbt_mint")
        .args_json(json!({"receiver": alice_acc.id(), "metadata": token_metadata, "memo": "test"}))
        .deposit(parse_near!("0.01 N"))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    let res = authority_acc
        .call(community_mainnet.id(), "sbt_mint")
        .args_json(json!({"receiver": bob_acc.id(), "metadata": token_metadata, "memo": "test"}))
        .deposit(parse_near!("1 N"))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    return Ok((
        registry_mainnet.clone(),
        community_mainnet.clone(),
        community_contract,
        authority_acc.clone(),
    ));
}

#[tokio::test]
async fn migration_mainnet() -> anyhow::Result<()> {
    let worker = workspaces::sandbox().await?;
    let (_, community_sbt, _, admin) = init(&worker).await?;

    // deploy the new contract
    let new_community_contract = community_sbt
        .deploy(include_bytes!("../../res/community_sbt.wasm"))
        .await?
        .into_result()?;

    // call the migrate method
    let res = new_community_contract
        .call("migrate")
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    // call the migration again should fail
    let res = new_community_contract
        .call("migrate")
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_failure());

    // call the contract after the migration
    let res: AccountId = new_community_contract
        .call("registry")
        .max_gas()
        .transact()
        .await?
        .json()?;
    let expected_res: AccountId = "registry-v1.gwg-testing.near".parse().unwrap();
    assert_eq!(expected_res, res);

    // change the admin
    let res = admin
        .call(new_community_contract.as_account().id(), "change_admin")
        .args_json(json!({"new_admin": "test.near"}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    // try to changing the admin again should fail
    let res = admin
        .call(new_community_contract.as_account().id(), "change_admin")
        .args_json(json!({"new_admin": "test.near"}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_failure());

    Ok(())
}
