# Example Apps for NEARCon 2022 Hackathon

This document demonstrates the Soulbound Token reference implementation, Human Class basic implementation and 2 use cases: _Universal Basic Income_ and a _Farm Drop_.

### Deploy an example SBT contract and Human Class

```bash
B=blacklist_accounts.i-am-human.testnet
S=sbt1.i-am-human.testnet
C=class1.i-am-human.testnet

near create-account $B --masterAccount i-am-human.testnet --initialBalance 4
near deploy $B target/wasm32-unknown-unknown/release/blacklist_human_addresses.wasm  "new" '{}'

near create-account $S --masterAccount i-am-human.testnet --initialBalance 7
near deploy $S target/wasm32-unknown-unknown/release/soulbound.wasm "new"  '{"issuer": "robertz.testnet", "operators": ["robertz.testnet"], "metadata": {"spec":"sbt-1.0.0", "name": "fist-bump", "symbol": "fist-bump-sbt"}, "blacklist_registry": "'$B'"}'

near create-account $C --masterAccount i-am-human.testnet --initialBalance 6
near deploy $C target/wasm32-unknown-unknown/release/soulbound_class.wasm "new" '{"required_sbt": "'$S'", "min_amount": 2}'
```

Now, we will use 2 test accounts: `alice_star.testnet` and `bob_star.testnet`. We will issue 2 tokens to Alice and one to Bob.

```bash
> near call $S sbt_mint '{"metadata": {"name":"fist band with Robert"}, "receiver": "alice_star.testnet"}' --account_id robertz.testnet --depositYocto 1
https://explorer.testnet.near.org/transactions/4PczF4tPHhsFT5V8zoyGH3PuYV2V4zqD2kDFLsZga9e8

> near call $S sbt_mint '{"metadata": {"name":"fist band with Noak"}, "receiver": "alice_star.testnet"}' --account_id robertz.testnet --depositYocto 1
https://explorer.testnet.near.org/transactions/9cWPogZGNRuPQrkPAn1gEyGFzjE9ccNEZHbGTxhmqLPb

> near call $S sbt_mint '{"metadata": {"name":"fist band with Noak"}, "receiver": "bob_star.testnet"}' --account_id robertz.testnet --depositYocto 1
https://explorer.testnet.near.org/transactions/GtbjrBTb6iei9FeXyC9W3bQcXXZrn5R93CSYFGYZ8cDd
```

Now we can verify that Alice is qualified in class1 and Bob is not:

```bash
> near call $C is_qualified '{"account": "alice_star.testnet"}' --account_id robertz.testnet
true
https://explorer.testnet.near.org/transactions/8JK7iSrf4BxUcBSBhZVkNqmpc1Ss2FLyQzsMBqnfZsfz

> near call $C is_qualified '{"account": "bob_star.testnet"}' --account_id robertz.testnet
false
https://explorer.testnet.near.org/transactions/5HyCLKMsMwCYp7p3skoXNE6NTLwhRvwwKXnChmFDnGtY


> near call $C is_qualified '{"account": "unknown.testnet"}' --account_id robertz.testnet
false
https://explorer.testnet.near.org/transactions/8UqsAX8H9bwBj6b2kbRZbVUaLXP236rboF3YTHgjJ2JR
```

### UBI

We will utilize `class1` Human Class to create a simple [Universal Basic Income](https://www.investopedia.com/terms/b/basic-income.asp) (UBI) smart contract.
The UBI smart contract implemented for this demo is a naive implementation:

- user to claim an UBI must firstly register himself.
  - during registration, we check if a user is qualified in `class1`. If not, the registration will fail.
- once a user is registered, he can call `claim` method to receive daily UBI.
- we will set 0.02 NEAR emission / day / user
- enough NEAR must be provided to the contract to cover storage and UBI emissions.

```bash
UBI=ubi.i-am-human.testnet
> near deploy $UBI target/wasm32-unknown-unknown/release/ubi.wasm "new" '{"human_class": "'$C'", "emission": "20000000000000000000000"}'
https://explorer.testnet.near.org/transactions/5AQM5iAQRsCvX2nSd7wLguuRM9zMfQbuoiLXmgc8KQ9t

near send i-am-human.testnet $UBI 5
```

We firstly register Alice - registration should work because she is qualified in `class1`. Right after we will try to claim the UBI.

```bash
> near call $UBI register '{}' --account_id alice_star.testnet --gas=100000000000000
true
https://explorer.testnet.near.org/transactions/88z7Lj4SpyeUibuRNg1CHw8cPkYKsscvs2RH1zdgo8o3

> near state alice_star.testnet | grep amount
  amount: '199995118013366575500000000',

> near call $UBI claim '{}' --account_id alice_star.testnet
https://explorer.testnet.near.org/transactions/5niaa1JaZHNruUMFSDzCJS1jrqf8vDEoDcJt8Si4xxL1
> near state alice_star.testnet | grep amount
  amount: '200013390038916953900000000',
```

Registering Bob fails because he is not qualified in `class1`:

```bash
> near call $UBI register '{}' --account_id bob_star.testnet --gas=100000000000000
false
https://explorer.testnet.near.org/transactions/CGENd24zkKM9DkbVXhDLUm6H6L3Y8hH5iynTQ28HzhFK
```

### Social Graph

Social Graph can be used to generate SBT tokens, proving positive pass of a social graph algorithm.

### Cheddar Farm Drop

As an alternative to popular AirDrops, we are designing a [Cheddar Farm](https://cheddar.farm)
to stake community token and farm new token:

- Only users who pass a human criteria can register to a Cheddar Farm Drops.
- Each Farm Drop defines a staking token and a reward token. Staking token is usually
  one of the ecosystem tokens, while farmed tokens is a new token which is supposed to
  be distributed.
- This way we create loyalty network:
  - new tokens has an access to a community of doers and humans.
  - new token can be a staking token in the future (which can drive a demand).
  - cheddar can be additional minted to support new projects entering to the NEAER market.
