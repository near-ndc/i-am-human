# Open Community SBT issuer

Based on SBT NEP: https://github.com/near/NEPs/pull/393. Early Design: https://hackmd.io/ZvgUeoF4SMGM4InswH0Dsg

This is a permissionless version of the [Community SBT](../community-sbt/README.md) contract: anyone can acquire a class to be a minter and designate other minters.

See root [README](../../README.md#testnet) for deployed smart contract addresses.

## Usage

Community SBT contract is designed for a communities with authority.
The contract is an SBT issuer and allows anyone to be a minter by acquiring a new class. The class restriction is implemented in the `sbt_mint` function.

The SBT minting and revoking can be only executed by an account which has _Minting Authority_, hence ideally, minter should be a DAO. Minting Authorities are set per class ID. Each class ID can has one more minter.

Only class admin can add or revoke minting authority.

### Become an Issuer

Anyone can become an issuer by acquiring permissionlessly a class. Class is just an ID associated to the account that acquired it. Any account can acquire many classes.

Once you acquire a class, you can add more admins and add or remove minters, update [class metadata](https://github.com/near/NEPs/blob/master/neps/nep-0393.md#smart-contract-interface). A minter will have a permission to mint on your behalves, but won't be able to add nor remove other minters.

To prevent spam, a payment is required, that is defined by the `const REGISTRATION_COST`.

```shell
# acquire a new class, set initial set of minters, and set max_ttl (maximum time for expire of
# newly minted SBTs to 100 days) attaching 2N payment.
near call CTR_ADDRESS acquire_next_class \
  '{"requires_iah": true, "minters": [MINTER_ADDRESS], "max_ttl": 8640000000, "metadata": {"name": "YOUR CLASS NAME"}}' \
  --deposit 0.1 --accountId ADMIN

near call CTR_ADDRESS set_class_metadata \
  '{"class": ClassId, "metadata": "{<Metadata JSON>}"}' --accountId ADMIN

near call CTR_ADDRESS add_minters \
  '{"class": ClassId, "minters": [MINTER2]' --accountId ADMIN

```

And anyone can query the class metadata:

```shell
near view CTR_ADDRESS class_metadata '{"class": ClassId}'
```

#### TTL

See [Community SBT](../community-sbt/README.md#ttl).

#### Minting

The mint function requires a deposit which is computed by the [`required_sbt_mint_deposit`](https://github.com/alpha-fi/i-am-human/blob/master/contracts/community-sbt/src/lib.rs#L158) function. The whole deposit is passed to the registry to cover the storage costs.
Metadata attributes:

- `expires_at` is be overwritten to `now + ttl`.
- `issued_at` is be overwritten to "now".
- `reference` and `reference_hash` are optional - it should be related to token characteristics. See [memo and metadata](#memo-and-metadata) guidelines.

```shell
near call CTR_ADDRESS sbt_mint \
  '{"receiver": "receipient.near",
    "metadata": {"class": 1, "reference": "link to token characteristics"},
    "memo": "optional operation info"}'  \
  --deposit 0.01 --accountId ADMIN
```

It is also possible to mint few tokens at once. In the example below, `recipient1` will get 1 token, `recipient2` will get 3 tokens. Note that one account can have at most one token of a give class (SBT standard doesn't allow one account to hold more than one token of the same class).

```shell
near call CTR_ADDRESS sbt_mint_many \
  '{"token_spec": [
       ["receipient1.near",
        [{"class": 1, "reference": "token1 ref"}]],
       ["receipient2.near",
        [{"class": 1, "reference": "token2 ref"}, {"class": 2, "reference": "token3 ref"}, {"class": 3, "reference": "token4 ref"}]]
    ],
    "memo": "optional operation info"}'  \
  --deposit 0.04 --gas 100000000000000 --accountId ADMIN
```

To query minting authorities of a given class call:

```shell
near view CTR_ADDRESS minting_authorities \
  '{"class": CLASS_ID}'
```

#### Query Registry

``` shell
near view $REGISTRY sbt_tokens_by_owner '{"account": "YOU", "issuer":"CTR_ADDRESS"}'
```



### Memo and Metadata

See [Guidelines for using metadata and minting memo field](../community-sbt/README.md#memo-and-metadata).
