# SBT Registry

The Registry smart contract is a balance book for all associated SBT tokens. The registry enables atomic `soul_transfers` and provides an efficient way to ban a smart contract (issuer). For more details check the [nep-393](https://github.com/near/NEPs/pull/393).

## SBT opt-in

Every SBT smart contract must opt-in to a registry, or implement registry functionality by it's own. Different registries may implement different mechanisms for opt-in.

This implementation requires an admin account (could be a DAO) to add an issuer to the registry, and as a consequence allow the issuer to use SBT registry methods.

## SBT mint

The minting process is a procedure where we assign a new token to the provided receiver and keep track of it in the registry. The `sbt_mint` method must be called by a issuer that is opted-in. Additional:

- each `TokenMetadata` provided must have a non zero `class`,
- enough `Near` must be attached to cover the registry storage cost must be provided.

The method will emit the [`Mint`](https://github.com/alpha-fi/i-am-human/blob/master/contracts/sbt/src/events.rs#L69) event when succesful. There might be a case when the token vector provided is too long, and the gas is not enought to cover the minting process, then it will panic with `out of gas`.

## Additional Queries

The IAH Registry supports the following extra queries, which are not part of the NEP-393 standard:

- `is_human(account: AccountId) -> Proof`, where proof is list of SBTs (represented as a list of issuers and issuer minted tokens). For example, if `alice` is a human because she has `fractal: class 1` token with `tokenID=24`, then the function returns `["<fractal issuer account>", [24]]`. If the account is not a human, then an empty proof is returned (empty list).

## Additional Transactions

- `is_human_call(account: AccountId, ctr: AccountId, function: String, args: Base64VecU8)` checks if the account is human (using `is_human` method). If yes, then makes a cross contract call: `ctr.function(args)`. Args are correctly expanded into function arguments. See the function documentation for more details.

## SBT Recovery Blacklist Registry
