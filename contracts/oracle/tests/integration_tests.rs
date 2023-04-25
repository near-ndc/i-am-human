use ed25519_dalek::Keypair;
use near_primitives::serialize::to_base64;
use near_sdk::json_types::U128;
use near_sdk::ONE_YOCTO;
use workspaces::{Account, AccountId, Contract,DevNetwork, Worker};
use workspaces::operations::Function;
use workspaces::result::ValueOrReceiptId;

async fn init(
    worker: &Worker<impl DevNetwork>,
    initial_balance: U128,
) -> anyhow::Result<(Contract, Account, Contract, Keypair)> {

    //registry admin (authority)
    let mut csprng = OsRng {};
    let keypair: Keypair = Keypair::generate(&mut csprng);
    let authority = worker.dev_create_account().await?;

     //1.deploy registry
    let registry = worker.dev_deploy(include_bytes!("../../target/wasm32-unknown-unknown/release/i_am_human_registry.wasm")).await?;

    //2.initialize registry
    let res = registry
        .call("new")
        .args_json(keypair.public.to_bytes().to_vec())
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    //3.deploy oracle
    let oracle_sbt = worker.dev_deploy(include_bytes!("../../target/wasm32-unknown-unknown/release/oracle-sbt.wasm")).await?;
    let contract_metadata = ContractMetadata {spec: "test-0.0.0".to_owned(), name: "test".to_owned(), symbol: "TEST".to_owned(), icon: None, base_uri: None, reference: None, reference_hash: None };

    //4. initialize oracle
    let res = oracle_sbt
        .call("new")
        .args_json((to_base64(regisrty.id()),contract_metadata, regisrty.id(),0, keypair.public.to_bytes().to_vec()))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    return Ok((registry, authority, oracle_sbt, keypair));
}

//Helper methods

/// @timestamp: in seconds
fn mk_claim(claimer: AccountId, timestamp: u64, external_id: &str) -> Claim {
    Claim {
        claimer: claimer,
        external_id: external_id.to_string(),
        timestamp,
    }
}

// returns b64 serialized claim and signature
fn sign_claim(c: &Claim, k: &Keypair) -> (String, String) {
    let c_bz = c.try_to_vec().unwrap();
    let sig = k.sign(&c_bz);
    let sig_bz = sig.to_bytes();
    (b64_encode(c_bz), b64_encode(sig_bz.to_vec()))
}



#[tokio::test]
async fn test_close_account_empty_balance() -> anyhow::Result<()> {
    let initial_balance = U128::from(parse_near!("10000 N"));
    let worker = workspaces::sandbox().await?;
    let (registry, authority, sbt_oracle, keypair) = init(&worker, initial_balance).await?;

    let claim = mk_claim("test.near", 100, "0x1");
    let (claim, signature) = sign_claim(&claim, &keypair);
    let res = sbt_oracle
        .call(sbt_oracle.id(), "sbt_mint")
        .args_json((claim, signature, None))
        .max_gas()
        .deposit(ONE_YOCTO)
        .transact()
        .await?;
    assert!(res.json::<bool>()?);

    Ok(())
}