# SBT opt-in

Every SBT smart contract must opt-in to a registry, or implement registry functionality by it's own. Different registries may implement different mechanisms for opt-in. 

This implementation requires an admin address (could be a DAO) to add an issuer to the registry, and as a consequence allow the issuer to use SBT registry methods.

# SBT mint

The minting process is a procedure where we asign a new token to the provided reciver and keep track of it in the registry. The `sbt_mint` method must be called by a issuer that is opted-in. Additionaly:

- each `TokenMetadata` provided must have a non zero `class`,
- enough `Near` to cover the registry storage cost must be provided.

The method will emit the `Mint` event when succesful. There might be a case when the token vector provided is too long, and the gas is not enought to cover the minting process, then it will panic with `out of gas`. 

# SBT Recovery Blacklist Registry
