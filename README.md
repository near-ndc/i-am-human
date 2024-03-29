# I am Human -- Proof of Humanity

Monorepository of contracts for the I am Human: proof of humanity protocol.

List of contracts:

- `registry`: implements the SBT Registry, documented in the [NEP-393](https://github.com/near/NEPs/pull/393/).
  See [Registry README](./contracts/README.md) for a detailed documentation about the I Am Human registry, examples and information about extra queries and methods.

Helper crates:

- `sbt`: set of traits, events and common functions for [NEP-393](https://github.com/near/NEPs/pull/393/) SBT Standard.
- `cost`: Common functions and constants to calculate gas and storage deposit for IAH registry calls.
- `human_checker`: Helper contract for integration tests. Notably, used for `is_human_call`.

Issuers:

- `demo-issuer`: basic SBT Issuer: contains a list of admins who are authorized to issue SBTs.
- `community-sbt`: Permissioned Community Issuer of SBT tokens.
- `community-open`: Permissionless Community Issuer of SBT tokens.
- `oracle`: SBT Issuer which relays on an off-chain authority signing claims for issuing SBTs.

work in progress:

- `soulbound-class`: An algebraic class of tokens to efficiently query if a user have required subset of tokens.
- `ubi`: demo use case implementing universal basic income.

## Example Flow

For details about creating and querying SBTs, and flow diagrams, see the [NEP-393](https://github.com/near/NEPs/pull/393/).

Actors:

- user: someone who wants to hold SBT
- issuer: a contract or an account which can issue SBTs and is whitelisted in a registry. Issuer is usually an entity which makes validation to mint a specific class of SBTs.
- registry: a contract which keeps balance of SBTs.

Whenever a new issuer is created, it needs a registry to mint tokens. Today, IAH registry is permissioned: the IAH Registry admin has to add a new issuer willing to mint tokens within IAH registry. In the future this may change and the process can be permission-less (any issuer will be able to mint tokens in the IAH registry).

Issuer calls `registry.sbt_mint` to mint new tokens. Each token must have specified class in it's metadata. See NEP-393 to learn more about SBT classes. The mint call panics, if a recipient already holds a token with a same class of a given issuer.

Anyone can query registry to check token supply or query tokens by issuer or by owner.

## Deployed contracts

### Mainnet

Production:

- **SBT registry**: `registry.i-am-human.near` @ registry/v1.8.0
- **Fractal**: `fractal.i-am-human.near` @ oracle/v1.2.0
  - verification pubkey base64: `"zqMwV9fTRoBOLXwt1mHxBAF3d0Rh9E9xwSAXR3/KL5E="`
- **Community SBTs**: `community.i-am-human.near` @ community-sbt/v5.0.0
  Max and default [TTL](./contracts/community-sbt/README.md#ttl) = 1year.
  classes: 1=OG, 2=NDC_Contributor, 3=NDC_Core_Contributors, 4=NDC_Champion, 5=NDC_Mod, 6=NDC_TechWG, 7=Creatives_DAO
- **Regens SBTs**: `issuer.regens.near` @ community-sbt/v5.0.0
  Max and default [TTL](./contracts/community-sbt/README.md#ttl) = 1year.
  classes: ProofOfRegen=1
- **Proof of Vibes**: `issuer.proofofvibes.near` @ community-sbt/v5.0.0
  Max and default [TTL](./contracts/community-sbt/README.md#ttl) = 1year.
  classes: Vibes=1

Mainnet Testing:

- `registry-v1.gwg-testing.near` @ registry/v1.8.0
  IAH issuer: `(fractal.i-am-human.near, [1])`

Deprecated:

- GoodDollar-SBT: `gooddollar-v1.i-am-human.near`.
  verification pubkey base64: `"zqMwV9fTRoBOLXwt1mHxBAF3d0Rh9E9xwSAXR3/KL5E="`

### Testnet

- **SBT registry**:
  Testnet registry is used to test the issuer behavior. For testing other integrations (eg polling, elections) use the testing-unstable version. Consult issuer contracts to validate which issuer is linked to which registry. We may consider adding migration to `registry-1` to make it compatible with the latest version.
  - `registry-v2.i-am-human.testnet` @ registry/v1.8.0 (same as the prod version)
  - `registry-unstable-v2.i-am-human.testnet` @ registry/v1.8.0
- **Demo SBT**: `sbt1.i-am-human.testnet` (the `demo_issuer` contract)
- **Fractal**: `fractal-v2.i-am-human.testnet` @ oracle/v1.2.0
  registry: `registry-1.i-am-human.testnet`; Verification pubkey base64: `FGoAI6DXghOSK2ZaKVT/5lSP4X4JkoQQphv1FD4YRto=`, `claim_ttl`: 3600ms, FV class: 1
- **Community-SBT**: `community-v2.i-am-human.testnet` @ community-sbt/v5.0.0
  registry: `registry-v2.i-am-human.testnet`
  classes: 1=OG, 2=NDC_Contributor, 3=NDC_Core_Contributors, 4=NDC_Champion, 5=NDC_Mod, 6=NDC_TechWG, 7=Creatives_DAO
  Max and default [TTL](./contracts/community-sbt/README.md#ttl) = 1year.
- **Open Community SBTs**: `CTR open-v1.i-am-human.testnet` @ community-open/v1.0.0

Deprecated:

- SBT Registry:
  - `registry.i-am-human.testnet`
  - `registry-1.i-am-human.testnet` @ release/v0.2
  - `registry-unstable.i-am-human.testnet` @ registry/v1.6.0
- **Fractal**: `i-am-human-staging.testnet` @ oracle/v1.0.1
  registry: `registry-1.i-am-human.testnet`; Verification pubkey base64: `FGoAI6DXghOSK2ZaKVT/5lSP4X4JkoQQphv1FD4YRto=`, `claim_ttl`: 3600ms, FV class: 1
- **Community-SBT**: `community-v1.i-am-human.testnet` @ community-sbt/v4.3.0
  registry: `registry-1.i-am-human.testnet`
  classes: 1=OG, 2=NDC_Contributor, 3=NDC_Core_Contributors, 4=NDC_Champion, 5=NDC_Mod, 6=NDC_TechWG, 7=Creatives_DAO
- GoodDollar SBT: `gooddollar-v1.i-am-human.testnet`
