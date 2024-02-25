# Open Community SBT issuer

Based on SBT NEP: https://github.com/near/NEPs/pull/393. Early Design: https://hackmd.io/ZvgUeoF4SMGM4InswH0Dsg

This is a permissionless version of the [Community SBT](../community-sbt/README.md) contract: anyone can acquire a class to be a minter and designate other minters.

See root [README](../../README.md#testnet) for deployed smart contract addresses.

## Usage

Community SBT contract is designed for a communities with authority.
The contract can mint tokens of multiple classes. The class restriction is implemented in the `sbt_mint` function.

The SBT minting and revoking can be only executed by an account which has _Minting Authority_, hence ideally it's assigned to a DAO. Minting Authorities are set per class ID. Each class ID can has one more minter.

Only admin can add or revoke minting authority.

#### TTL

Time To Live (TTL) is a duration in milliseconds used to define token expire time: `expires_at = now + ttl`.
Every token class has its own `MAX_TTL` value which is being set when enabling new class for minting.
The `max_ttl` value can be changed by an admin by calling the `set_max_ttl` method.

#### SBT classes

SBT contract supports multiple token classes: one issuer can mint tokens of many classes.
The `community-sbt` contract requires an admin to enable a token class and set if minting of SBT of that class requires IAH humanity check. Moreover, admin must assign a minting authority (an address which is authorized to mint).

```shell
near call CTR_ADDRESS enable_next_class \
  '{"requires_iah": true, "minter": MINTER_ADDRESS}' --accountId ADMIN
```

Contract admin should set the metadata information for each class using:

```shell
near call CTR_ADDRESS set_class_metadata \
  '{"class": ClassId, "metadata": "Metadata JSON"}' --accountId ADMIN
```

And anyone can query the class metadata:

```shell
near view CTR_ADDRESS class_metadata '{"class": ClassId}'
```

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

### Memo and Metadata

Guidelines for using metadata and minting memo field.

Both `sbt_mint` and `sbt_mint_many` provide an optional `memo` argument which should be used as a reference for minting (operation data), usually a justification for a minting or a link to a Near Social post for justification. `memo` is not going to be recorded in the token.

If you want to record extra data to the token, then you should set it as a token `metadata.reference` (usually a JSON or a link to a JSON document). That should be related to a token, hence be a part of the token characteristic rather then the mint operation.

There is also contract metadata and class metadata - both are managed by the contract admin.

- [Contract Metadata](https://github.com/alpha-fi/i-am-human/blob/master/contracts/sbt/src/metadata.rs) should describe the contract and the issuer as well as common data to all token classes.
- [Class Metadata](https://github.com/alpha-fi/i-am-human/blob/master/contracts/sbt/src/metadata.rs) should describe the class and all common data for all tokens of that class. For example, token characteristics shared by all tokens of a given class should be set in the class metadata, rather than copied over all token metadata. Examples include icon, symbol etc...
