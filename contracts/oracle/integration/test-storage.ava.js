import { Worker, NEAR } from "near-workspaces";
import { Base64 } from 'js-base64';
import test from "ava";

test.beforeEach(async (t) => {
  // Init the worker and start a Sandbox server
  const worker = await Worker.init();

  // Prepare sandbox for tests, create accounts, deploy contracts, etc.
  const root = worker.rootAccount;
  const registry_contract = await root.createSubAccount('registry')
  const oracle_contract = await root.createSubAccount('oracle');
  const admin = await root.createSubAccount('admin');
  const claimer = await root.createSubAccount('claimer');
  const keypair = await admin.getKey();
  const claim_b64 = "EQAAAGNsYWltZXIudGVzdC5uZWFyBAAAADB4MWELAAAAAAAAAA==";
  const claim = Base64.decode(claim_b64);
  const sig = Base64.encode(keypair.sign(Uint8Array.from(claim)).signature);
  const sig_b64 = "c1z1yG+nnatk47PcN4IN5mqM90YkVb6S/dzVE0IzWPRHMeBmEZAz39pZL5T5YLvLI9kTj4f/HymfLA/3F9GsCQ==";
  // Deploy and initialize registry
  await registry_contract.deploy("/Users/stanislawczembor/i-am-human/contracts/oracle/build/wasm32-unknown-unknown/release/i_am_human_registry.wasm");
  await registry_contract.call(registry_contract, "new", {'authority': admin.accountId });

  // Deploy and initialize oracle
  await oracle_contract.deploy("/Users/stanislawczembor/i-am-human/contracts/oracle/build/wasm32-unknown-unknown/release/oracle_sbt.wasm");
  const metadata = {spec: "test", name: "test", symbol: "TEST"};
  const authority_base64 = Base64.encode(keypair.getPublicKey().data);
  await oracle_contract.call(oracle_contract, "new", 
   {'authority': "+9Yuc5NCUOhxLeW+HoXIhn7r5Qvo66+uTshO0losqVw",
    'metadata': metadata,
    'registry': registry_contract.accountId,
    'claim_ttl': 100000000000,
    'admin': admin.accountId, });

  // Save state for test runs, it is unique for each test
  t.context.worker = worker;
  t.context.accounts = { registry_contract, oracle_contract, admin, claim_b64, sig_b64, claimer };
});

test.afterEach(async (t) => {
  await t.context.worker.tearDown().catch((error) => {
    console.log("Failed tear down the worker:", error);
  });
});

test("Should fail, oracle is not an issuer", async (t) => {
  const {oracle_contract, claim_b64, sig_b64, claimer } = t.context.accounts;
  const sbt_id = await claimer.call(oracle_contract, "sbt_mint",
    { 'claim_b64': claim_b64,
      'claim_sig' : sig_b64 },
    { attachedDeposit: NEAR.parse("0.008 N").toString() },
    { gas: 200000000000000 }).catch((error) => { console.log('Transaction error:', error);});
  t.is(sbt_id, null);
});

test("Should pass, oracle is an issuer", async (t) => {
  const { registry_contract, oracle_contract, admin, claim_b64, sig_b64, claimer } = t.context.accounts;
  const result1 = await admin.call(registry_contract, "admin_add_sbt_issuer", {'issuer': oracle_contract.accountId});
  t.is(result1, true);
  const sbt_id =  await claimer.call(oracle_contract, "sbt_mint",
    { 'claim_b64': claim_b64,
      'claim_sig' : sig_b64 },
    { attachedDeposit: NEAR.parse("0.008 N").toString() },
    { gas: 200000000000000 }).catch((error) => { console.log('Transaction error:', error);});
  t.not(sbt_id, undefined);
});