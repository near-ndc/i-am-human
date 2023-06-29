# I am Human -- Proof of Humanity

Monorepository of contracts for the I am Human: proof of humanity protocol.

List of contracts:

- `sbt`: set of traits, events and common functions for [NEP-393](https://github.com/near/NEPs/pull/393/) SBT Standard.
- `registry`: implements the SBT Registry, documented in the [NEP-393](https://github.com/near/NEPs/pull/393/)
- `oracle`: SBT Issuer which relays on an off-chain authority signing claims for issuing SBTs.
- `demo-issuer`: basic SBT Issuer: contains a list of admins who are authorized to issue SBTs.

work in progress:

- `community-sbt`: Community Issuer of SBT tokens
- `soulbound-class`: An algebraic class of tokens to efficiently query if a user have required subset of tokens.
- `ubi`: demo use case implementing universal basic income.

## Example Flow

For details about creating and querying SBTs, and flow diagrams, see the [NEP-393](https://github.com/near/NEPs/pull/393/).

Actors:

- user: someone who wants to hold SBT
- issuer: a contract or an account which can issue SBTs and is whitelisted in a registry. Issuer is usually an entity which makes validation to mint a specific class of SBTs.
- registry: a contract which keeps balance of SBTs.

Whenever a new issuer is created, it needs a registry to mint tokens. Today, IAH registry is permissioned: the IAH Registry admin has to add a new issuer willing to mint tokens within IAH registry. In the future this may change and the process can be permissionless (any issuer will be able to mint tokens in the IAH registry).

Issuer calls `registry.sbt_mint` to mint new tokens. Each token must have specified class in it's metadata. See NEP-393 to learn more about SBT classes. The mint call panics, if a recipient already holds a token with a same class of a given issuer.

Anyone can query registry to check token supply or query tokens by issuer or by owner.

### Additional Queries

The IAH Registry supports the following extra queries, which are not part of the NEP-393 standard:

- `is_human(account: AccountId) -> bool`: returns true, if the given account is not human as specified by the registry criteria.

## Deployed contracts

### Mainnet

- **SBT registry**: `registry.i-am-human.near` @ registry/v1.0.0
- **Fractal**: `fractal.i-am-human.near` @ oracle/v1.0.0
  - verification pubkey base64: `"zqMwV9fTRoBOLXwt1mHxBAF3d0Rh9E9xwSAXR3/KL5E="`
- **Community SBTs**: `community.i-am-human.near` @ community-sbt/v2.0.0
  - OG class: 1

Deprecated:

- **GoodDollar-SBT**: `gooddollar-v1.i-am-human.near`.
  verification pubkey base64: `"zqMwV9fTRoBOLXwt1mHxBAF3d0Rh9E9xwSAXR3/KL5E="`

### Testnet

- **SBT registry**: `registry-1.i-am-human.testnet`
- **Demo SBT Issuer**: `sbt1.i-am-human.testnet` (the `demo_issuer` contract)
- **Fractal Issuer**: `i-am-human-staging.testnet` (the `oracle` contract). Verification pubkey base64: `zqMwV9fTRoBOLXwt1mHxBAF3d0Rh9E9xwSAXR3/KL5E=`, `claim_ttl`: 3600ms
  - FV class: 1
- **Community-SBT**: `community-v1.i-am-human.testnet`
  - OG class: 1

Deprecated:

- `registry.i-am-human.testnet`
- GoodDollar SBT: `gooddollar-v1.i-am-human.testnet`
