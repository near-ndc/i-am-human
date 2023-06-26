# Proof of concept for Community SBT

Based on SBT NEP: https://github.com/near/NEPs/pull/393. Early Design: https://hackmd.io/ZvgUeoF4SMGM4InswH0Dsg

See root [README](../../README.md#testnet) for deployed smart contract addresses.

## Usage

Community SBT contract is designed for a communities with authority. The authority is has a minting and revoking power, hence ideally it's assigned to a DAO.

The contract can mint tokens of multiple classes. The class restriction is implemented in the `sbt_mint` function.

#### Minting

Metadata attributes:

- `expires_at` is be overwritten to `now + ttl` (ttl is the parameter set to in the contract constructor).
- `issued_at` is be overwritten to "now".
- `reference` and `reference_hash` are optional.

```shell
near call <ctr-address> sbt_mint '{"receiver": "receipient.near", "metadata": {"class": 1, "reference": "near-social-post-link"}}'  --deposit 0.008 --accountId <dao address>
```

## GWG Community SBT

Minting is currently restricted to one class: `1` (OG).
TTL is set to 1 year.
