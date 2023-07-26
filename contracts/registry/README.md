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

- `is_human(account: AccountId) -> Proof`, where proof is list of SBTs (represented as a list of issuers and issuer minted tokens). The registry has a property `iah_sbts` that specifies which tokens from which issuers are required from an account to be considered a human. In case the account is missing any of the required tokens an empty proof will be returned (empty list).
  For example, if `alice` is a human because she has `fractal: class 1` token with `tokenID=24`, then the function returns `["<fractal issuer account>", [24]]`. If the account is not a human, then an empty proof is returned (empty list). If the `iah_sbts` property contains more tokens, for example `fratcal: [1,2]` the `is_human` will return the proof with the tokens only if the account has both of the sbts. Otherwise an empty proof will be returned. eg. for `alice` with two tokens `class=1, tokenID=24` and `class=2, tokenID=40` the method will return `["<fractal issuer account>", [24, 40]]`. for `bob` with one token `class=1, tokenID=26` the method will return an empty list.

- `iah_class_set() -> ClassSet` - returns IAH class set: required token classes to be approved as a human by the `is_human`.

## Additional Transactions

- `sbt_mint_iah(token_spec: Vec<(AccountId, Vec<TokenMetadata>)>) -> Vec<TokenId>` is a wrapper around `sbt_mint` and `is_human`. It mints SBTs only when all recipients are humans.
- `is_human_call(ctr: AccountId, function: String, payload: JSONString)` checks if the predecessor account (_caller_) account is human (using `is_human` method). If it's not, then it panics and returns the deposit. Otherwise it makes a cross contract call passing the deposit:
  ```
  ctr.function(caller=predecessor_account_id,
               iah_proof=tokens_prooving_caller_humanity,
               payload)
  ```
  Classical example will registering an action (for poll participation), only when a user is a human.
  Instead of `Poll --is_human--> Registry -> Poll`, we can simplify and do `Registry.is_human_call --> Poll`.
  See the function documentation for more details.
- `sbt_burn(issuer: AccountId, tokens: Vec<TokenId>, memo: Option<String>)` - every holder can burn some of his tokens.
