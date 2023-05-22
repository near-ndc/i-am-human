# SBT opt-in

Every SBT smart contract that wants to relay the state changes on the registry must opt-in into the registry. Different registries may implement different mechanisms for opt-in. In the current implementation the only way to opt-in as an issuer is by calling `admin_add_sbt_issuer`. The transaction must be signed by the admin.

# SBT mint

The minting process is a procedure where we asign a new token to the provided reciver and keep track of it in the registry. The `sbt_mint` method must be called by a issuer that is opted-in. Additionaly:

- each `TokenMetadata` provided must have a non zero `class`,
- enough `Near` to cover the registry storage cost must be provided.

The method will emit the `Mint` event when succesful. There might be a case when the token vector provided is too long, and the gas is not enought to cover the minting process, then it will panic with `out of gas`. 

# SBT Recovery Blacklist Registry
