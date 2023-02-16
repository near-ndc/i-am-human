# Oracle SBT

Status: Proof of Concept
Based on SBT NEP: https://github.com/near/NEPs/pull/393

See root [README](../../README.md#testnet) for deployed smart contract addresses.

The Oracle SBT mints SBT based on a single authority oracle. This is used to provide NEAR SBT as a representation of externally verified identity (Verisoul, GoodDollar, KYC...).

Overview:

- During contract initialization, authority public key (`authority_pubkey`) is set. It's used to verify signatures.
- User requests an SBT off-chain by interacting with the oracle authority to obtain a valid `Claim`.
- The `Claim` contains his AccountId and associated ID related to his external identity.
- authority, off chain, upon user authentication, signs the `Claim` with a private key associated to the `authority_pubkey`.
- Once having signed `Claim`, user makes `sbt_issue` transaction with base64 borsh serialized Claim to self mint SBT.
- Upon successful `Claim` verification SBT is minted:
  - `Claim` signature is correct
  - `Claim.user == transaction signer`
  - `Claim.timestamp` is not later than `now + claim_ttl`
  - `Claim.external_id` nor `Claim.user` has not been used.
