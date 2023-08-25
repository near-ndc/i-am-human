use std::str::FromStr;
use workspaces::network::{NetworkClient, NetworkInfo, Sandbox};
use workspaces::result::ExecutionSuccess;
use workspaces::{
    types::{Balance, KeyType, SecretKey},
    Account, Contract, DevNetwork, Worker,
};

// Generate user sub-account
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

// Build contract from sources and initialize it
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

// Load already built contract and initialize it
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

// Get current block timestamp
pub async fn get_block_timestamp<T>(worker: &Worker<T>) -> anyhow::Result<u64>
where
    T: NetworkClient + Send + Sync,
{
    Ok(worker.view_block().await?.timestamp())
}
