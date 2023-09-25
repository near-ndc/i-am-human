use anyhow::Ok;
use near_units::parse_near;
use sbt::TokenMetadata;
use serde_json::json;
use std::str::FromStr;
use workspaces::network::{NetworkClient, NetworkInfo, Sandbox};
use workspaces::result::ExecutionSuccess;
use workspaces::AccountId;
use workspaces::{
    types::{Balance, KeyType, SecretKey},
    Account, Contract, DevNetwork, Worker,
};

pub mod oracle;
pub mod utils;

/// Generate user sub-account
pub async fn gen_user_account<T>(worker: &Worker<T>, account_id: &str) -> anyhow::Result<Account>
where
    T: DevNetwork + Send + Sync,
{
    let id = workspaces::AccountId::from_str(account_id)?;
    let sk = SecretKey::from_random(KeyType::ED25519);

    let account = worker.create_tla(id, sk).await?.into_result()?;

    Ok(account)
}

pub async fn transfer_near(
    worker: &Worker<Sandbox>,
    account_id: &workspaces::AccountId,
    deposit: Balance,
) -> anyhow::Result<ExecutionSuccess> {
    Ok(worker
        .root_account()?
        .transfer_near(account_id, deposit)
        .await?
        .into_result()?)
}

/// Build contract from sources and initialize it
pub async fn build_contract<T>(
    worker: &Worker<T>,
    project_path: &str,
    init_method: &str,
    args: near_sdk::serde_json::Value,
) -> anyhow::Result<Contract>
where
    T: NetworkInfo + NetworkClient + DevNetwork + Send + Sync,
{
    let wasm = workspaces::compile_project(project_path).await?;

    let (id, sk) = worker.dev_generate().await;

    let contract = worker
        .create_tla_and_deploy(id.clone(), sk, &wasm)
        .await?
        .into_result()?;

    // initialize contract
    let _ = contract
        .call(init_method)
        .args_json(args)
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    Ok(contract)
}

/// Load already built contract and initialize it
pub async fn load_contract<T>(
    worker: &Worker<T>,
    contract_path: &str,
    init_method: &str,
    args: near_sdk::serde_json::Value,
) -> anyhow::Result<Contract>
where
    T: NetworkInfo + NetworkClient + DevNetwork + Send + Sync,
{
    let wasm = std::fs::read(contract_path)?;
    let (id, sk) = worker.dev_generate().await;

    let contract = worker
        .create_tla_and_deploy(id, sk, &wasm)
        .await?
        .into_result()?;

    // initialize contract
    let _ = contract
        .call(init_method)
        .args_json(args)
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    Ok(contract)
}

/// Get current block timestamp
pub async fn get_block_timestamp<T>(worker: &Worker<T>) -> anyhow::Result<u64>
where
    T: NetworkClient + Send + Sync,
{
    Ok(worker.view_block().await?.timestamp())
}

/// Helper function to issue tokens to the users for testing purposes
pub async fn registry_mint_iah_tokens(
    registry: &AccountId,
    issuer: &Account,
    class_id: u64,
    accounts: Vec<&AccountId>,
) -> anyhow::Result<()> {
    // populate registry with mocked data
    let token_metadata = vec![TokenMetadata {
        class: class_id,
        issued_at: Some(0),
        expires_at: None,
        reference: None,
        reference_hash: None,
    }];
    let mut iah_token_spec = Vec::new();

    for a in accounts {
        iah_token_spec.push((a, token_metadata.clone()));
    }

    let res = issuer
        .call(registry, "sbt_mint")
        .args_json(json!({ "token_spec": iah_token_spec }))
        .deposit(parse_near!("5 N"))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success(), "{:?}", res.receipt_failures());

    Ok(())
}

/// Helper function to add issuers to the registry
pub async fn registry_add_issuer(
    registry: &AccountId,
    authority: &Account,
    issuers: Vec<&AccountId>,
) -> anyhow::Result<()> {
    for i in issuers {
        let res = authority
            .call(registry, "admin_add_sbt_issuer")
            .args_json(json!({ "issuer": i }))
            .max_gas()
            .transact()
            .await?;
        assert!(res.is_success());
    }
    Ok(())
}

// Helper function to deploy, initalize and mint iah sbts to the `iah_accounts`.
pub async fn registry_default<T>(
    worker: &Worker<T>,
    authority: &AccountId,
    flaggers: Vec<&AccountId>,
    iah_accounts: Vec<&AccountId>,
) -> anyhow::Result<(Account, Account)>
where
    T: DevNetwork + Send + Sync,
{
    const IAH_CLASS: u64 = 1;
    let iah_issuer = worker.dev_create_account().await?;
    let registry_contract = build_contract(
        &worker,
        "./../registry",
        "new",
        json!({"authority": authority, "authorized_flaggers": flaggers, "iah_issuer": iah_issuer.id(), "iah_classes": [IAH_CLASS]}),
    ).await?;

    // issue iah tokens to iah_accounts
    registry_mint_iah_tokens(registry_contract.id(), &iah_issuer, 1, iah_accounts).await?;

    Ok((registry_contract.as_account().clone(), iah_issuer))
}
