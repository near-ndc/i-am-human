use anyhow::Ok;
use near_units::parse_near;
use near_workspaces::{network::Sandbox, result::ExecutionFinalResult, Account, Contract, Worker};
use sbt::{SBTs, TokenMetadata};
use serde_json::json;

use human_checker::{RegisterHumanPayload, VotePayload, VOTING_DURATION};

const REGISTER_HUMAN_TOKEN: &str = "register_human_token";
const MSECOND : u64 = 1000;

struct Suite {
    registry: Contract,
    human_checker: Contract,
}

impl Suite {
    pub async fn is_human_call(
        &self,
        caller: &Account,
        payload: &RegisterHumanPayload,
    ) -> anyhow::Result<ExecutionFinalResult> {
        let res = caller
        .call(self.registry.id(), "is_human_call")
        .args_json(json!({"ctr": self.human_checker.id(), "function": REGISTER_HUMAN_TOKEN, "payload": serde_json::to_string(payload).unwrap()}))
        .max_gas()
        .transact()
        .await?;
        println!(">>> is_human_call logs {:?}\n", res.logs());
        Ok(res)
    }

    pub async fn is_human_call_lock(
        &self,
        caller: &Account,
        lock_duration: u64,
        payload: &VotePayload,
    ) -> anyhow::Result<ExecutionFinalResult> {
        let res = caller
        .call(self.registry.id(), "is_human_call_lock")
        .args_json(json!({"ctr": self.human_checker.id(), "function": "vote", "payload": serde_json::to_string(payload).unwrap(), "lock_duration": lock_duration, "with_proof": false}))
        .max_gas()
        .transact()
        .await?;
        println!(">>> is_human_call_lock logs {:?}\n", res.logs());
        Ok(res)
    }


    pub async fn query_sbts(&self, user: &Account) -> anyhow::Result<Option<SBTs>> {
        // check the key does not exists in human checker
        let r = self
            .human_checker
            .call("recorded_sbts")
            .args_json(json!({"user": user.id()}))
            .max_gas()
            .transact()
            .await?;
        let result: Option<SBTs> = r.json()?;
        Ok(result)
    }
}

async fn init(
    worker: &Worker<Sandbox>,
) -> anyhow::Result<(Contract, Contract, Account, Account, Account, Account)> {
    // import the contract from mainnet
    let registry = worker
        .dev_deploy(include_bytes!("../../res/registry.wasm"))
        .await?;
    let human_checker = worker
        .dev_deploy(include_bytes!("../../res/human_checker.wasm"))
        .await?;

    let authority = worker.dev_create_account().await?;
    let auth_flagger = worker.dev_create_account().await?;
    let iah_issuer = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;
    let bob = worker.dev_create_account().await?;
    let john = worker.dev_create_account().await?;

    // init the contracts
    let res1 = registry
        .call("new")
        .args_json(json!({"authority": authority.id(),
                   "iah_issuer": iah_issuer.id(), "iah_classes": [1],
                   "authorized_flaggers": vec![auth_flagger.id()]}))
        .max_gas()
        .transact();

    let res2 = human_checker
        .call("new")
        .args_json(json!({"registry": registry.id()}))
        .max_gas()
        .transact();

    assert!(res1.await?.is_success() && res2.await?.is_success());

    // add iah_issuer
    let res = authority
        .call(registry.id(), "admin_add_sbt_issuer")
        .args_json(json!({"issuer": iah_issuer.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    // populate registry with mocked data
    let iah_token_spec = vec![
        (
            alice.id(),
            vec![TokenMetadata {
                class: 1,
                issued_at: Some(0),
                expires_at: None,
                reference: None,
                reference_hash: None,
            }],
        ),
        (
            bob.id(),
            vec![TokenMetadata {
                class: 2,
                issued_at: Some(0),
                expires_at: None,
                reference: None,
                reference_hash: None,
            }],
        ),
    ];

    let res = iah_issuer
        .call(registry.id(), "sbt_mint")
        .args_json(json!({ "token_spec": iah_token_spec }))
        .deposit(parse_near!("0.1 N"))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    Ok((
        registry,
        human_checker.clone(),
        alice,
        bob,
        john,
        iah_issuer,
    ))
}

#[tokio::test]
async fn is_human_call() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let (registry, human_checker, alice, bob, john, issuer) = init(&worker).await?;
    let issuer_id = near_sdk::AccountId::try_from(issuer.id().as_str().to_owned())?;

    let payload = RegisterHumanPayload {
        memo: "registering alice".to_owned(),
        numbers: vec![2, 3, 5, 7, 11],
    };

    let suite = Suite {
        registry,
        human_checker,
    };

    // Call using Alice. Should register tokens, because Alice is a human
    let r = suite.is_human_call(&alice, &payload).await?;
    assert!(r.is_success());
    let result: bool = r.json()?; // the final receipt is register_human_token, which return boolean
    assert!(result, "should register tokens to alice");

    let mut tokens = suite.query_sbts(&alice).await?;
    assert_eq!(tokens, Some(vec![(issuer_id, vec![1])]));

    // call the is_human_call method with bob (has sbts but not a human)
    // should panic in the human_checker
    let r = suite.is_human_call(&bob, &payload).await?;
    assert!(r.is_failure());

    tokens = suite.query_sbts(&bob).await?;
    assert_eq!(tokens, None);

    // call the is_human_call method john (doesn't have sbts)
    // should panic in the registry
    let r = suite.is_human_call(&john, &payload).await?;
    assert!(r.is_failure());

    tokens = suite.query_sbts(&john).await?;
    assert_eq!(tokens, None);


    //
    // Test Vote with lock duration
    //

    //
    // test1: too short lock duration: should fail
    let mut payload = VotePayload{prop_id: 10, vote: "approve".to_string()};
    let r = suite.is_human_call_lock(&alice, VOTING_DURATION / 3 *2,  &payload).await?;
    assert!(r.is_failure());
    let failure_str = format!("{:?}",r.failures());
    assert!(failure_str.contains("sufficient amount of time"), "{}", failure_str);

    //
    // test2: second call, should not change
    let r = suite.is_human_call_lock(&alice, VOTING_DURATION / 3*2,  &payload).await?;
    assert!(r.is_failure());
    let failure_str = format!("{:?}",r.failures());
    assert!(failure_str.contains("sufficient amount of time"), "{}", failure_str);

    //
    // test3: longer call should be accepted, but should fail on wrong payload (vote option)
    payload.vote = "wrong-wrong".to_string();
    let r = suite.is_human_call_lock(&alice, VOTING_DURATION +MSECOND,  &payload).await?;
    assert!(r.is_failure());
    let failure_str = format!("{:?}",r.failures());
    assert!(failure_str.contains("invalid vote: must be either"), "{}", failure_str);

    //
    // test4: should work with correct input
    payload.vote = "approve".to_string();
    let r = suite.is_human_call_lock(&alice, VOTING_DURATION +MSECOND,  &payload).await?;
    assert!(r.is_success());

    //
    // test5: should fail with not a human
    let r = suite.is_human_call_lock(&john, VOTING_DURATION + MSECOND,  &payload).await?;
    assert!(r.is_failure());
    let failure_str = format!("{:?}",r.failures());
    assert!(failure_str.contains("is not a human"), "{}", failure_str);

    Ok(())
}
