use near_units::parse_near;
use sbt::TokenMetadata;
use serde_json::json;
use workspaces::{Account, DevNetwork, Worker};

async fn init(worker: &Worker<impl DevNetwork>) -> anyhow::Result<Account> {
    // deploy the old contract
    let (registry, regsitry_sk) = worker.dev_generate().await;
    let registry_mainnet = worker
        .create_tla(registry, regsitry_sk)
        .await?
        .into_result()?;
    let registry_contract = registry_mainnet
        .deploy(include_bytes!("../../res/registry-v1-mainnet.wasm"))
        .await?
        .into_result()?;

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

    return Ok(registry_mainnet);
}
#[tokio::test]
async fn migration() -> anyhow::Result<()> {
    let worker = workspaces::testnet().await?;
    let registry_mainnet = init(&worker).await?;

    // deploy the new contract
    let new_registry_mainnet = registry_mainnet
        .deploy(include_bytes!("../../res/registry.wasm"))
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
    Ok(())
}
