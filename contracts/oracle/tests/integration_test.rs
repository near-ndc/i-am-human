use std::str::FromStr;

use near_sdk::ONE_NEAR;
use serde_json::json;
use test_util::{
    build_contract,
    common::ExternalAccountId,
    gen_user_account,
    utils::{build_signed_claim, generate_keys},
};
use workspaces::{Account, AccountId, Contract, DevNetwork, Worker};

use oracle_sbt::{MINT_TOTAL_COST, MINT_TOTAL_COST_WITH_KYC};
use sbt::ContractMetadata;

const AUTHORITY_KEY: &str = "zqMwV9fTRoBOLXwt1mHxBAF3d0Rh9E9xwSAXR3/KL5E=";
const CLAIM_TTL: u64 = 3600 * 24 * 365 * 100;

async fn init(worker: &Worker<impl DevNetwork>) -> anyhow::Result<(Contract, Account, Account)> {
    // deploy contracts
    let oracle = worker.dev_deploy(include_bytes!("../../res/oracle_sbt.wasm"));
    let registry = worker.dev_deploy(include_bytes!("../../res/registry.wasm"));

    let oracle = oracle.await?;
    let registry = registry.await?;

    let alice = worker.dev_create_account().await?;
    let admin = worker.dev_create_account().await?;
    let auth_flagger = worker.dev_create_account().await?;

    //
    // we are usign same setup as in claim_sig_and_sbt_mint unit test
    //

    let res1 = oracle
        .call("new")
        .args_json(json!({
            "authority": AUTHORITY_KEY,
            "admin": admin.id(),
            "registry": registry.id(),
            "claim_ttl": CLAIM_TTL,
            "metadata": ContractMetadata{spec: "sbt".to_owned(), name: "oracle".to_owned(), symbol: "iah".to_owned(), icon: None, base_uri: None, reference: None, reference_hash: None},
        }))
        .max_gas()
        .transact();

    let res2 = registry
        .call("new")
        .args_json(json!({
            "authority": admin.id(),
            "iah_issuer": oracle.id(), "iah_classes": [1],
            "authorized_flaggers": vec![auth_flagger.id()]}))
        .max_gas()
        .transact();

    let res1 = res1.await?;
    let res2 = res2.await?;

    assert!(res1.is_success(), "res oracle {:?}", res1);
    assert!(res2.is_success(), "res registry {:?}", res2);

    // get current block time
    // let block = worker.view_block().await?;
    // let now = block.timestamp() / MSECOND; // timestamp in seconds

    Ok((oracle.to_owned(), admin, alice))
}

#[tokio::test]
async fn check_arithmetic_exception_dev() -> anyhow::Result<()> {
    let worker = workspaces::sandbox().await?;
    let (oracle, _, alice) = init(&worker).await?;
    check_arithmetic_exception(oracle, alice).await?;

    Ok(())
}

#[ignore]
#[tokio::test]
async fn check_arithmetic_exception_mainnet() -> anyhow::Result<()> {
    let worker = workspaces::sandbox().await?;
    let worker_mainnet = workspaces::mainnet_archival().await?;

    let oracle_id: AccountId = "fractal.i-am-human.near".parse()?;
    const BLOCK_HEIGHT: u64 = 97933983; // this is around when the claims start to fail in the mainnet

    let oracle = worker
        .import_contract(&oracle_id, &worker_mainnet)
        .initial_balance(1000 * ONE_NEAR)
        .block_height(BLOCK_HEIGHT)
        //.with_data()
        .transact()
        .await?;

    // we can't import data because it's too big, so we need to initialize the contract
    let res1 = oracle
        .call("new")
        .args_json(json!({
            "authority": AUTHORITY_KEY,
            "admin": "admin.near",
            "registry": "registry.near",
            "claim_ttl": CLAIM_TTL,
            "metadata": ContractMetadata{spec: "sbt".to_owned(), name: "oracle".to_owned(), symbol: "iah".to_owned(), icon: None, base_uri: None, reference: None, reference_hash: None},
        }))
        .max_gas()
        .transact().await?;
    assert!(res1.is_success(), "res oracle {:?}", res1);

    // create and fund alice account
    let alice_root = worker.dev_create_account().await?;
    let alice_tx = alice_root.create_subaccount("alice").transact().await?;
    assert!(
        alice_tx.is_success(),
        "alice tx: {:?}\n",
        alice_tx.details.receipt_failures()
    );
    let alice = alice_tx.result;
    let tx = alice_root.transfer_near(alice.id(), ONE_NEAR).await?;
    assert!(tx.is_success(), "transfer: {:?}\n", tx.outcomes());

    check_arithmetic_exception(oracle, alice).await?;

    Ok(())
}

#[tokio::test]
async fn test_mint_sbt() -> anyhow::Result<()> {
    let worker = workspaces::sandbox().await?;

    let (sec_key, pub_key) = generate_keys();

    let authority = gen_user_account(&worker, "admin.test.near").await?;
    let iah_issuer = gen_user_account(&worker, "iah_issuer.test.near").await?;
    let flagger = gen_user_account(&worker, "flagger.test.near").await?;

    let registry_contract = build_contract(
        &worker,
        "../registry/",
        "new",
        json!({"authority": authority.id(), "authorized_flaggers": [flagger.id()], "iah_issuer": iah_issuer.id(), "iah_classes": [1]})
    )
    .await?;

    let oracle_contract = build_contract(
        &worker,
        "../oracle/",
        "new",
        json!({
            "authority": near_sdk::base64::encode(pub_key.unwrap_as_ed25519().as_ref()),
            "metadata": {
                "spec": "v1.0.0",
                "name": "test-sbt",
                "symbol": "SBT"
            },
            "registry": registry_contract.id(),
            "claim_ttl": 100000000000u64,
            "admin": authority.id(),
        }),
    )
    .await?;

    let user_account = gen_user_account(&worker, "user.test.near").await?;
    let signed_claim = build_signed_claim(
        near_sdk::AccountId::from_str(user_account.id().as_str())?,
        ExternalAccountId::gen(),
        false,
        &sec_key,
    )?;

    // TODO: add check for specific error text
    let _ = user_account
        .call(oracle_contract.id(), "sbt_mint")
        .args_json(signed_claim)
        .max_gas()
        .transact()
        .await?
        .into_result()
        .expect_err("only root and implicit accounts are allowed to get SBT");

    let user_account = worker.root_account()?;
    let signed_claim = build_signed_claim(
        near_sdk::AccountId::from_str(user_account.id().as_str())?,
        ExternalAccountId::gen(),
        false,
        &sec_key,
    )?;

    // TODO: add check for specific error text
    let _ = user_account
        .call(oracle_contract.id(), "sbt_mint")
        .args_json(signed_claim)
        .max_gas()
        .transact()
        .await?
        .into_result()
        .expect_err("Requires attached deposit of exactly 0.008 NEAR");

    let signed_claim = build_signed_claim(
        near_sdk::AccountId::from_str(user_account.id().as_str())?,
        ExternalAccountId::gen(),
        true,
        &sec_key,
    )?;

    // TODO: add check for specific error text
    let _ = user_account
        .call(oracle_contract.id(), "sbt_mint")
        .args_json(&signed_claim)
        .max_gas()
        .transact()
        .await?
        .into_result()
        .expect_err("Requires attached deposit of exactly 0.015 NEAR");

    // TODO: add check for specific error text
    let _ = user_account
        .call(oracle_contract.id(), "sbt_mint")
        .args_json(json!({
            "claim_b64": signed_claim.claim_b64,
            "claim_sig": format!("a{}", &signed_claim.claim_sig),
        }))
        .deposit(MINT_TOTAL_COST_WITH_KYC)
        .max_gas()
        .transact()
        .await?
        .into_result()
        .expect_err("can't base64-decode claim_sig");

    let user_account = worker.root_account()?;
    let signed_claim = build_signed_claim(
        near_sdk::AccountId::from_str(user_account.id().as_str())?,
        ExternalAccountId::gen(),
        false,
        &sec_key,
    )?;

    let _ = user_account
        .call(oracle_contract.id(), "sbt_mint")
        .args_json(signed_claim)
        .deposit(MINT_TOTAL_COST)
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    Ok(())
}

async fn check_arithmetic_exception(oracle: Contract, alice: Account) -> anyhow::Result<()> {
    //
    // replicating claim_sig_and_sbt_mint unit test
    // in testnet it fails with with "WebAssembly trap: An arithmetic exception, e.g. divided by zero."
    //   https://explorer.testnet.near.org/transactions/GobWuBgA9HLsUk4UTtVqrSiyy24P6B2cnywLfeh9mdtv
    // however, the claim and transactions are correctly signed.
    // If verification is correct it should fail with "claimer is not a transaction signer" because
    // we are submitting the claim using a different account.

    println!(">>>> account: {}\n", alice.id());

    let claim_b64 = "FAAAAG15YWNjb3VudDEyMy50ZXN0bmV0IAAAAGFmZWU5MmYwNzEyMjQ2NGU4MzEzYWFlMjI1Y2U1YTNmSGa2ZAAAAAAA";
    let claim_sig_b64 =
        "38X2TnWgc6moc4zReAJFQ7BjtOUlWZ+i3YQl9gSMOXwnm5gupfHV/YGmGPOek6SSkotT586d4zTTT2U8Qh3GBw==";
    let res = alice
        .call(oracle.id(), "sbt_mint")
        .args_json(json!({"claim_b64": claim_b64, "claim_sig": claim_sig_b64}))
        .deposit(MINT_TOTAL_COST)
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_failure());
    let output = format!("{:?}", res.receipt_failures());
    assert!(
        output
            .as_str()
            .contains("claimer is not the transaction signer"),
        "{}",
        output
    );

    Ok(())
}
