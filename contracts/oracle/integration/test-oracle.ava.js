import { Worker, NEAR, Gas } from "near-workspaces";
import test from "ava";

const claim_b64 = "EQAAAGNsYWltZXIudGVzdC5uZWFyBAAAADB4MWELAAAAAAAAAAA="; //base64 of Claim 
const sig_b64 = "6163iDaU8LNe+uiihqs+cwXQHd/wPwBcXBHiD02Bdp/Y/+/R8/Ev1kMwNZPRTKXb/q84zNy7eOdoljIM2i0nCw=="; //base64 of signature of Claim 

const claim_b64_with_kyc = "EQAAAGNsYWltZXIudGVzdC5uZWFyBAAAADB4MWELAAAAAAAAAAE="; //base64 of Claim 
const sig_b64_with_kyc = "jwjzUJYiIsCSsnxbNhV8zXrjY7UvWN4e3d9nQePJohbwYw7iaMen65zShn3DO7r1C+ZQv179KoJabduSxCbzDw=="; //base64 of signature of Claim 

const external_id = "0x1a";

test.beforeEach(async (t) => {
  // Init the worker and start a Sandbox server
  const worker = await Worker.init();

  // Prepare sandbox for tests, create accounts, deploy contracts, etc.
  const root = worker.rootAccount;
  const registry = await root.createSubAccount('registry')
  const oracle = await root.createSubAccount('oracle');
  const admin = await root.createSubAccount('admin');
  const claimer = await root.createSubAccount('claimer');
  // Deploy and initialize registry
  await registry.deploy("../res/registry.wasm");
  await registry.call(registry, "new", {'authority': admin.accountId });

  // Deploy and initialize oracle
  await oracle.deploy("../res/oracle_sbt.wasm");
  const sbtMetadata = {spec: "v1.0.0", name: "test-sbt", symbol: "SBT"};
  await oracle.call(oracle, "new", 
   {'authority': "1npXqp38AmvmWL3ZkC4Y/Cts5yb3od7ZnHeQUxWWpDU=", //base64 of authority pub key used for claim signature authorization
    'metadata': sbtMetadata,
    'registry': registry.accountId,
    'claim_ttl': 100000000000,
    'admin': admin.accountId, });

  // Save state for test runs, it is unique for each test
  t.context.worker = worker;
  t.context.accounts = { registry_contract: registry, oracle_contract: oracle, admin, claimer };
});

test.afterEach(async (t) => {
  await t.context.worker.tearDown().catch((error) => {
    console.log("Failed tear down the worker:", error);
  });
});

test("Should fail: mint sbt, oracle is not an issuer", async (t) => {
  const {oracle_contract, claimer } = t.context.accounts;
  const mint_result = await claimer.call(oracle_contract, "sbt_mint",
    { 'claim_b64': claim_b64,
      'claim_sig' : sig_b64 },
    { attachedDeposit: NEAR.parse("0.008 N").toString() },
    { gas: Gas.parse('20 Tgas') }).catch((error) => { console.log('Transaction error:', error);});
  t.deepEqual(mint_result, {Err: 'registry.sbt_mint failed'});
  const is_used_identity = await oracle_contract.view("is_used_identity", { 'external_id': external_id});
  t.false(is_used_identity);
});

test("Should pass: mint sbt, oracle is an issuer", async (t) => {
  const { registry_contract, oracle_contract, admin, claimer } = t.context.accounts;
  const add_issuer_result = await admin.call(registry_contract, "admin_add_sbt_issuer", {'issuer': oracle_contract.accountId});
  t.is(add_issuer_result, true);
  const mint_result =  await claimer.call(oracle_contract, "sbt_mint",
    { 'claim_b64': claim_b64,
      'claim_sig' : sig_b64 },
    { attachedDeposit: NEAR.parse("0.008 N").toString() },
    { gas: Gas.parse('20 Tgas') }).catch((error) => { console.log('Transaction error:', error);});
  t.not(mint_result, undefined);
  const is_used_identity = await oracle_contract.view("is_used_identity", { 'external_id': external_id});
  t.true(is_used_identity);
});

test("Should pass: mint sbt token and revoke (burn)", async (t) => {
  const { registry_contract, oracle_contract, admin, claimer } = t.context.accounts;
  const add_issuer_result = await admin.call(registry_contract, "admin_add_sbt_issuer", {'issuer': oracle_contract.accountId});
  t.is(add_issuer_result, true);
  let supply_by_issuer = await registry_contract.view("sbt_supply", {'issuer': oracle_contract.accountId});
  t.assert(supply_by_issuer === 0);
  const mint_result =  await claimer.call(oracle_contract, "sbt_mint",
    { 'claim_b64': claim_b64,
      'claim_sig' : sig_b64 },
    { attachedDeposit: NEAR.parse("0.008 N").toString() },
    { gas: Gas.parse('20 Tgas') }).catch((error) => { console.log('Transaction error:', error);});
  t.not(mint_result, undefined);
  const is_used_identity = await oracle_contract.view("is_used_identity", { 'external_id': external_id});
  t.true(is_used_identity);
  supply_by_issuer = await registry_contract.view("sbt_supply", {'issuer': oracle_contract.accountId});
  t.assert(supply_by_issuer === 1);
  await admin.call(oracle_contract, "sbt_revoke", {'tokens': [mint_result.Ok], 'burn': true}, { gas: Gas.parse('20 Tgas') });
  supply_by_issuer = await registry_contract.view("sbt_supply", {'issuer': oracle_contract.accountId});
  t.assert(supply_by_issuer === 0);
})

test("Should pass: mint sbt token and kyc token", async (t) => {
  //TODO: add integration test when verified_kyc == true
  const { registry_contract, oracle_contract, admin, claimer } = t.context.accounts;
  const add_issuer_result = await admin.call(registry_contract, "admin_add_sbt_issuer", {'issuer': oracle_contract.accountId});
  t.is(add_issuer_result, true);
  let supply_by_issuer = await registry_contract.view("sbt_supply", {'issuer': oracle_contract.accountId});
  t.assert(supply_by_issuer === 0);
  const mint_result =  await claimer.call(oracle_contract, "sbt_mint",
    { 'claim_b64': claim_b64_with_kyc,
      'claim_sig' : sig_b64_with_kyc },
    { attachedDeposit: NEAR.parse("0.015 N").toString() },
    { gas: Gas.parse('20 Tgas') }).catch((error) => { console.log('Transaction error:', error);});
  t.not(mint_result, undefined);
  console.log("mint result",mint_result.Ok);
  const is_used_identity = await oracle_contract.view("is_used_identity", { 'external_id': external_id});
  t.true(is_used_identity);
  supply_by_issuer = await registry_contract.view("sbt_supply", {'issuer': oracle_contract.accountId});
  t.assert(supply_by_issuer === 2);
})