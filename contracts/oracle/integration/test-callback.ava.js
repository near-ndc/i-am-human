import { Worker, NEAR } from "near-workspaces";
import test from "ava";

const claim_b64 = "EQAAAGNsYWltZXIudGVzdC5uZWFyBAAAADB4MWELAAAAAAAAAA=="; //base64 of Claim 
const sig_b64 = "c1z1yG+nnatk47PcN4IN5mqM90YkVb6S/dzVE0IzWPRHMeBmEZAz39pZL5T5YLvLI9kTj4f/HymfLA/3F9GsCQ=="; //base64 of signature of Claim 
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
  await registry.deploy("../res/i_am_human_registry.wasm");
  await registry.call(registry, "new", {'authority': admin.accountId });

  // Deploy and initialize oracle
  await oracle.deploy("../res/oracle_sbt.wasm");
  const sbtMetadata = {spec: "v1.0.0", name: "test-sbt", symbol: "SBT"};
  await oracle.call(oracle, "new", 
   {'authority': "+9Yuc5NCUOhxLeW+HoXIhn7r5Qvo66+uTshO0losqVw", //base64 of authority pub key used for claim signature authorization
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

test("Should fail, oracle is not an issuer", async (t) => {
  const {oracle_contract, claimer } = t.context.accounts;
  const mint_result = await claimer.call(oracle_contract, "sbt_mint",
    { 'claim_b64': claim_b64,
      'claim_sig' : sig_b64 },
    { attachedDeposit: NEAR.parse("0.008 N").toString() },
    { gas: Gas.parse('20 Tgas') }).catch((error) => { console.log('Transaction error:', error);});
  t.deepEqual(mint_result, {Err: 'registry.sbt_mint failed'});
  const is_used_identity = await claimer.call(oracle_contract, "is_used_identity", { 'external_id': external_id}, { gas: 200000000000000 });
  t.false(is_used_identity);
});

test("Should pass, oracle is an issuer", async (t) => {
  const { registry_contract, oracle_contract, admin, claimer } = t.context.accounts;
  const add_issuer_result = await admin.call(registry_contract, "admin_add_sbt_issuer", {'issuer': oracle_contract.accountId});
  t.is(add_issuer_result, true);
  const mint_result =  await claimer.call(oracle_contract, "sbt_mint",
    { 'claim_b64': claim_b64,
      'claim_sig' : sig_b64 },
    { attachedDeposit: NEAR.parse("0.008 N").toString() },
    { gas: Gas.parse('20 Tgas') }).catch((error) => { console.log('Transaction error:', error);});
  t.not(mint_result, undefined);
  const is_used_identity = await claimer.call(oracle_contract, "is_used_identity", { 'external_id': external_id}, { gas: 200000000000000 });
  t.true(is_used_identity);
});
