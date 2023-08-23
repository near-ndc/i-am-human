use anyhow::Ok;
use near_units::parse_near;
use sbt::{ClassMetadata, Token, TokenMetadata};
use serde_json::json;
use workspaces::{network::Sandbox, Account, AccountId, Contract, Worker};

const MAINNET_REGISTRY_ID: &str = "registry-v1.gwg-testing.near";
const MAINNET_COMMUNITY_SBT_ID: &str = "community-testing.i-am-human.near";

async fn init(
    worker: &Worker<Sandbox>,
    migration: bool,
) -> anyhow::Result<(Account, Account, Contract, Account, Account)> {
    let registry_contract: Contract;
    let community_contract: Contract;
    if migration {
        // import the registry contract from mainnet
        let worker_mainnet = workspaces::mainnet().await?;
        let contract_id: AccountId = MAINNET_REGISTRY_ID.parse()?;
        registry_contract = worker
            .import_contract(&contract_id, &worker_mainnet)
            .initial_balance(parse_near!("10000000 N"))
            .transact()
            .await?;

        // import the community-sbt contract from mainnet
        let contract_id: AccountId = MAINNET_COMMUNITY_SBT_ID.parse()?;
        community_contract = worker
            .import_contract(&contract_id, &worker_mainnet)
            .initial_balance(parse_near!("10000000 N"))
            .transact()
            .await?;
    } else {
        registry_contract = worker
            .dev_deploy(include_bytes!("../../res/registry.wasm"))
            .await?;
        community_contract = worker
            .dev_deploy(include_bytes!("../../res/community_sbt.wasm"))
            .await?;
    }

    let registry_mainnet = registry_contract.as_account();
    let community_mainnet = community_contract.as_account();
    let authority = worker.dev_create_account().await?;
    let auth_flagger = worker.dev_create_account().await?;
    let minter = worker.dev_create_account().await?;
    let iah_issuer = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;
    let bob = worker.dev_create_account().await?;

    // init the registry
    let res = registry_contract
        .call("new")
        .args_json(json!({"authority": authority.id(),
                          "iah_issuer": iah_issuer.id(), "iah_classes": [1],
                          "authorized_flaggers": vec![auth_flagger.id()] }))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    // init the community-sbt
    let res = community_contract
        .call("new")
        .args_json(
            json!({"registry": registry_mainnet.id(), "admin": authority.id(), "metadata": {
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
    let res = authority
        .call(registry_mainnet.id(), "admin_add_sbt_issuer")
        .args_json(json!({"issuer": iah_issuer.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    // add community_issuer
    let res = authority
        .call(registry_mainnet.id(), "admin_add_sbt_issuer")
        .args_json(json!({"issuer": community_mainnet.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    if migration {
        // authorize authority to mint tokens
        let res = authority
            .call(community_mainnet.id(), "enable_next_class")
            .args_json(json!({"requires_iah": false, "minter": minter.id(), "memo": "test"}))
            .max_gas()
            .transact()
            .await?;
        assert!(res.is_success());
    } else {
        let res = authority
            .call(community_mainnet.id(), "enable_next_class")
            .args_json(
                json!({"requires_iah": false, "minter": minter.id(),"max_ttl": 100000000,
                       "metadata": ClassMetadata {
                           name: "cls-1".to_string(),
                           symbol: None,
                           icon: None,
                           reference: None,
                           reference_hash: None},
                       "memo": "test"}),
            )
            .max_gas()
            .transact()
            .await?;
        assert!(res.is_success(), "{:?}", res.receipt_failures());

        let res = authority
            .call(community_mainnet.id(), "enable_next_class")
            .args_json(
                json!({"requires_iah": false, "minter": minter.id(),"max_ttl": 100000000,
                       "metadata": ClassMetadata {
                           name: "cls-2".to_string(),
                           symbol: None,
                           icon: None,
                           reference: None,
                           reference_hash: None},
                       "memo": "test"}),
            )
            .max_gas()
            .transact()
            .await?;
        assert!(res.is_success(), "{:?}", res.receipt_failures());
    }

    // mint mocked community tokens
    let token_metadata = TokenMetadata {
        class: 1,
        issued_at: Some(0),
        expires_at: None,
        reference: None,
        reference_hash: None,
    };

    let res = minter
        .call(community_mainnet.id(), "sbt_mint")
        .args_json(json!({"receiver": alice.id(), "metadata": token_metadata, "memo": "test"}))
        .deposit(parse_near!("0.01 N"))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    let res = minter
        .call(community_mainnet.id(), "sbt_mint")
        .args_json(json!({"receiver": bob.id(), "metadata": token_metadata, "memo": "test"}))
        .deposit(parse_near!("0.01 N"))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    return Ok((
        registry_mainnet.clone(),
        community_mainnet.clone(),
        community_contract,
        authority.clone(),
        minter.clone(),
    ));
}

#[tokio::test]
async fn migration_mainnet() -> anyhow::Result<()> {
    let worker = workspaces::sandbox().await?;
    let (_, community_sbt, _, admin, _) = init(&worker, true).await?;

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

    // authorize authority to mint tokens
    let res = admin
        .call(new_community_contract.id(), "enable_next_class")
        .args_json(
            json!({"requires_iah": true, "minter": admin.id(),"max_ttl": 2147483647,
                   "metadata": ClassMetadata {
                       name: "cls-1".to_string(),
                       symbol: None,
                       icon: None,
                       reference: None,
                       reference_hash: None},
                   "memo": "test"}),
        )
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

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

#[tokio::test]
async fn sbt_renew() -> anyhow::Result<()> {
    let worker = workspaces::sandbox().await?;
    let (registry, community_sbt, _, admin, minter) = init(&worker, false).await?;

    let sbts: Vec<Option<Token>> = admin
        .call(registry.id(), "sbts")
        .args_json(json!({"issuer": community_sbt.id(), "tokens": [1,2]}))
        .max_gas()
        .transact()
        .await?
        .json()?;
    let sbt1_ttl_before_renew = sbts[0].as_ref().unwrap().metadata.expires_at.unwrap();
    let sbt2_ttl_before_renew = sbts[1].as_ref().unwrap().metadata.expires_at.unwrap();

    let res = minter
        .call(community_sbt.id(), "sbt_renew")
        .args_json(json!({"tokens": [1,2], "ttl": 100000000, "memo": "test"}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    let sbts: Vec<Option<Token>> = admin
        .call(registry.id(), "sbts")
        .args_json(json!({"issuer": community_sbt.id(), "tokens": [1,2]}))
        .max_gas()
        .transact()
        .await?
        .json()?;

    // check if the renew updated the ttl
    assert!(sbts[0].as_ref().unwrap().metadata.expires_at.unwrap() > sbt1_ttl_before_renew);
    assert!(sbts[1].as_ref().unwrap().metadata.expires_at.unwrap() > sbt2_ttl_before_renew);

    // renew non existing tokens
    let res = minter
        .call(community_sbt.id(), "sbt_renew")
        .args_json(json!({"tokens": [3,4], "ttl": 200000, "memo": "test"}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_failure());

    Ok(())
}

#[tokio::test]
async fn sbt_renew_fail() -> anyhow::Result<()> {
    let worker = workspaces::sandbox().await?;
    let (registry, community_sbt, _, admin, _) = init(&worker, false).await?;

    let sbts: Vec<Option<Token>> = admin
        .call(registry.id(), "sbts")
        .args_json(json!({"issuer": community_sbt.id(), "tokens": [1,2]}))
        .max_gas()
        .transact()
        .await?
        .json()?;
    let sbt1_ttl_before_renew = sbts[0].as_ref().unwrap().metadata.expires_at.unwrap();
    let sbt2_ttl_before_renew = sbts[1].as_ref().unwrap().metadata.expires_at.unwrap();

    // should fail since the admin is not a minter
    let res = admin
        .call(community_sbt.id(), "sbt_renew")
        .args_json(json!({"tokens": [1,2], "ttl": 100000000, "memo": "test"}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_failure());

    let sbts: Vec<Option<Token>> = admin
        .call(registry.id(), "sbts")
        .args_json(json!({"issuer": community_sbt.id(), "tokens": [1,2]}))
        .max_gas()
        .transact()
        .await?
        .json()?;

    // check if the ttl is still the same
    assert_eq!(
        sbts[0].as_ref().unwrap().metadata.expires_at.unwrap(),
        sbt1_ttl_before_renew
    );
    assert_eq!(
        sbts[1].as_ref().unwrap().metadata.expires_at.unwrap(),
        sbt2_ttl_before_renew
    );

    // renew non existing tokens
    let res = admin
        .call(community_sbt.id(), "sbt_renew")
        .args_json(json!({"tokens": [3,4], "ttl": 200000, "memo": "test"}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_failure());

    Ok(())
}

#[tokio::test]
async fn sbt_revoke() -> anyhow::Result<()> {
    let worker = workspaces::sandbox().await?;
    let (registry, community_sbt, _, admin, minter) = init(&worker, false).await?;

    let sbts: Vec<Option<Token>> = admin
        .call(registry.id(), "sbts")
        .args_json(json!({"issuer": community_sbt.id(), "tokens": [1,2]}))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert!(sbts.len() == 2);
    assert!(sbts[0].is_some());
    assert!(sbts[1].is_some());

    // Should pass since community_sbt is minter
    let res = minter
        .call(community_sbt.id(), "sbt_revoke")
        .args_json(json!({"tokens": [1,2], "burn": true, "memo": "test"}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    let sbts: Vec<Option<Token>> = admin
        .call(registry.id(), "sbts")
        .args_json(json!({"issuer": community_sbt.id(), "tokens": [1,2]}))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert!(sbts.len() == 2);
    assert!(sbts[0].is_none());
    assert!(sbts[1].is_none());

    // revoke non existing tokens
    let res = admin
        .call(community_sbt.id(), "sbt_revoke")
        .args_json(json!({"tokens": [3,4], "burn": true, "memo": "test"}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_failure());

    Ok(())
}

#[tokio::test]
async fn sbt_revoke_fail() -> anyhow::Result<()> {
    let worker = workspaces::sandbox().await?;
    let (registry, community_sbt, _, admin, _) = init(&worker, false).await?;

    let sbts: Vec<Option<Token>> = admin
        .call(registry.id(), "sbts")
        .args_json(json!({"issuer": community_sbt.id(), "tokens": [1,2]}))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert!(sbts.len() == 2);
    assert!(sbts[0].is_some());
    assert!(sbts[1].is_some());

    // should fail since the admin is not a minter
    let res = admin
        .call(community_sbt.id(), "sbt_revoke")
        .args_json(json!({"tokens": [1,2], "burn": true, "memo": "test"}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_failure());

    let sbts: Vec<Option<Token>> = admin
        .call(registry.id(), "sbts")
        .args_json(json!({"issuer": community_sbt.id(), "tokens": [1,2]}))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert!(sbts.len() == 2);
    assert!(sbts[0].is_some());
    assert!(sbts[1].is_some());

    Ok(())
}

#[tokio::test]
async fn sbt_mint_many() -> anyhow::Result<()> {
    let worker = workspaces::sandbox().await?;
    let (registry, community_sbt, _, admin, minter) = init(&worker, false).await?;

    let bob = worker.dev_create_account().await?;
    let charlie = worker.dev_create_account().await?;

    let supply: u64 = admin
        .call(registry.id(), "sbt_supply")
        .args_json(json!({"issuer": community_sbt.id()}))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(supply, 2);

    let token_metadata = vec![
        TokenMetadata {
            class: 1,
            issued_at: Some(0),
            expires_at: None,
            reference: None,
            reference_hash: None,
        },
        TokenMetadata {
            class: 2,
            issued_at: Some(0),
            expires_at: None,
            reference: None,
            reference_hash: None,
        },
    ];

    // sbt_mint_many
    let res = minter
        .call(community_sbt.id(), "sbt_mint_many")
        .args_json(json!({
            "token_spec":
                vec![
                    (bob.id(), token_metadata.clone()),
                    (charlie.id(), token_metadata),
                ]
        }))
        .deposit(parse_near!("0.1 N"))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success(), "{:?}", res.receipt_failures());

    let supply: u64 = admin
        .call(registry.id(), "sbt_supply")
        .args_json(json!({"issuer": community_sbt.id()}))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(supply, 6);

    Ok(())
}
