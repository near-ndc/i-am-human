use near_units::parse_near;
use sbt::TokenMetadata;
use serde_json::json;
use workspaces::{Account, Contract, DevNetwork, Worker};

async fn init(
    worker: &Worker<impl DevNetwork>,
    wasm_path: &str,
) -> anyhow::Result<(Account, Account, Contract)> {
    // deploy the old contract
    let (registry_pk, regsitry_sk) = worker.dev_generate().await;
    let registry_mainnet = worker
        .create_tla(registry_pk, regsitry_sk)
        .await?
        .into_result()?;
    let wasm = std::fs::read(wasm_path)?;
    let registry_contract = registry_mainnet.deploy(&wasm).await?.into_result()?;

    let authority_acc = worker.dev_create_account().await?;
    let iah_issuer = worker.dev_create_account().await?;
    let alice_acc = worker.dev_create_account().await?;
    let bob_acc = worker.dev_create_account().await?;
    let john_acc = worker.dev_create_account().await?;
    let elon_acc = worker.dev_create_account().await?;

    // init the contract
    let res = registry_contract
        .call("new")
        .args_json(json!({"authority": authority_acc.id()}))
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

    // populate registry with mocked data
    let token_metadata = vec![TokenMetadata {
        class: 1,
        issued_at: Some(0),
        expires_at: None,
        reference: None,
        reference_hash: None,
    }];

    let token_spec = vec![
        (alice_acc.id(), token_metadata.clone()),
        (bob_acc.id(), token_metadata.clone()),
        (john_acc.id(), token_metadata.clone()),
        (elon_acc.id(), token_metadata),
    ];

    let res = iah_issuer
        .call(registry_mainnet.id(), "sbt_mint")
        .args_json(json!({ "token_spec": token_spec }))
        .deposit(parse_near!("1 N"))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    return Ok((registry_mainnet, iah_issuer, registry_contract));
}

#[tokio::test]
async fn migration_mainnet() -> anyhow::Result<()> {
    let worker = workspaces::sandbox().await?;
    let (registry_mainnet, issuer, old_registry_contract) =
        init(&worker, "./tests/contracts/registry-v1-mainnet.wasm").await?;

    let supply: u64 = old_registry_contract
        .call("sbt_supply")
        .args_json(json!({"issuer": issuer.id()}))
        .max_gas()
        .transact()
        .await?
        .json()?;

    // deploy the new contract
    let new_registry_mainnet = registry_mainnet
        .deploy(include_bytes!("contracts/registry.wasm"))
        .await?
        .into_result()?;

    // call the migrate method
    let res = new_registry_mainnet
        .call("migrate")
        .args_json(json!({"iah_issuer": "iah-issuer.testnet", "iah_classes": [1]}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    let res: u64 = new_registry_mainnet
        .call("sbt_supply")
        .args_json(json!({"issuer": issuer.id()}))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(res, supply);

    Ok(())
}
