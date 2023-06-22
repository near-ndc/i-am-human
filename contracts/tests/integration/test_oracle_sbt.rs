use near_sdk::serde_json::json;
use oracle_sbt::{MINT_TOTAL_COST, MINT_TOTAL_COST_WITH_KYC};
use std::str::FromStr;
use tests::common::ExternalAccountId;
use tests::utils::{build_signed_claim, generate_keys};
use tests::workspaces::{build_contract, gen_user_account};

#[tokio::test]
async fn test_mint_sbt() -> anyhow::Result<()> {
    let worker = workspaces::sandbox().await?;

    let (sec_key, pub_key) = generate_keys();

    let admin_account = gen_user_account(&worker, "admin.test.near").await?;

    let registry_contract = build_contract(
        &worker,
        "../registry/",
        json!({
            "authority": admin_account.id(),
        }),
    )
    .await?;

    let oracle_contract = build_contract(
        &worker,
        "../oracle/",
        json!({
            "authority": near_sdk::base64::encode(pub_key.unwrap_as_ed25519().as_ref()),
            "metadata": {
                "spec": "v1.0.0",
                "name": "test-sbt",
                "symbol": "SBT"
            },
            "registry": registry_contract.id(),
            "claim_ttl": 100000000000u64,
            "admin": admin_account.id(),
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
