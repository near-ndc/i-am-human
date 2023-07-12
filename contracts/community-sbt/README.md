# Proof of concept for Community SBT

Based on SBT NEP: https://github.com/near/NEPs/pull/393. Early Design: https://hackmd.io/ZvgUeoF4SMGM4InswH0Dsg

See root [README](../../README.md#testnet) for deployed smart contract addresses.

## Usage

Community SBT contract is designed for a communities with authority.
The contract can mint tokens of multiple classes. The class restriction is implemented in the `sbt_mint` function.

The SBT minting and revoking can be only executed by an account which has _Minting Authority_, hence ideally it's assigned to a DAO. Minting Authorities are set per class ID. Each class ID can has one more minter.

Only admin can add or revoke minting authority.

#### TTL

Time To Live (TTL) is a duration in milliseconds used to define token expire time: `expires_at = now + ttl`
The constructor parameter is used to define max and default TTL when minting tokens.

#### SBT classes

SBT contract supports token classes: one issuer can mint tokens of many classes.
The `community-sbt` contract requires an admin to enable class and set if minting of SBT tokens of that class require IAH humanity check. Moreover, admin must assign a minting authority (an address which is authorized to mint).

```shell
near call CTR_ADDRESS enable_next_class \
  '{"requires_iah": true, "minter": MINTER_ADDRESS}' \
  --accountId ADMIN
```

#### Minting

The mint function requires a deposit which is computed by the (`required_sbt_mint_deposit`)[https://github.com/alpha-fi/i-am-human/blob/master/contracts/community-sbt/src/lib.rs#L158] function. The whole deposit is passed to the registry to cover the storage costs.
Metadata attributes:

- `expires_at` is be overwritten to `now + ttl`.
- `issued_at` is be overwritten to "now".
- `reference` and `reference_hash` are optional.

```shell
near call CTR_ADDRESS sbt_mint \
  '{"receiver": "receipient.near", "metadata": {"class": 1, "reference": "near-social-post-link"}}'  \
  --deposit 0.008 --accountId ADMIN
```

To query minting authorities of a given class call:

```shell
near view CTR_ADDRESS minting_authorities \
  '{"class": CLASS_ID}'
```
