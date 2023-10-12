# Oracle SBT

Status: Proof of Concept
Based on SBT NEP: https://github.com/near/NEPs/pull/393

See root [README](../../README.md#testnet) for deployed smart contract addresses.

The Oracle SBT mints SBT based on an authority oracle. This is used to provide NEAR SBT as a representation of externally verified identity (Fractal, Verisoul, GoodDollar, KYC...).

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

## Design

Currently the Oracle smart contract is used to mint SBTs based Fractal ID verification. User can receive Fractal FV SBT (class=1) if s/he passed Fractal Face Scan verification, and additionally Fractal KYC (class=2) if s/he passed KYC check.

The goal of the oracle contract is to attest off chain fact, and mint SBT. The general logic is following:

1. User has an external (off the NEAR blockchain) account, with properties we want to attest. Examples:

   - [GoodDollar verified account](https://help.gooddollar.org/kb/getting-started/how-to-complete-face-verification-process), used for verifying unique humans to receive $GD UBI
   - Twitter blue checkmark
   - GitCoin passport
   - Fractal ID, attesting various verification levels: Face Scan, Proof of Address...

2. The `oracle` smart contract provides a logic to attest property of an externally owned account in order to validate SBT minting request. The [`sbt_mint`](https://github.com/alpha-fi/i-am-human/blob/master/contracts/oracle/src/lib.rs#L120) function requires a [`Claim`](https://github.com/alpha-fi/i-am-human/blob/master/contracts/oracle/src/util.rs#L14) and `claim_sig`

3. `claim_sig` is the ed25519 signature of the Claim bytes (using Borsh serialization).

   - Smart contract contains `authority_pubkey`, which is used to validate the signature. Only Claim signed by the approved authority are accepted by the smart contract.
   - For Fractal ID verification, we are using the [verification oracle](https://github.com/near-ndc/verification-oracle) as the service creating and signing the claims.

4. The Claim is created (and signed) by an external service, and provided to `sbt_mint` function as Borsh serialized and then base64 into the string format. The `Claim` provides all data required to properly attest an external account:

   - `claimer`: a NEAR account that is a subject of the claim.
   - `external_id`: an external account identity. The oracle contract makes sure that each external identity is used only once. - `timestamp`: Unix Timestamp (in seconds) when the claim is made.
   - `verified_kyc`: property custom to the application of the oracle contract for NDC GWG: flag checking if the claim

5. In the current version of the oracle, the property we are attesting is implicit - meaning we don't explicitly set it in the Claim. Instead it's subsumed by the flow and the `Claim.verified_kyc`. The smart contract checks that `external_id` is used only once, hence the current version doesn't support claims attesting different properties.
   So, it's not possible to make a 2 different claims about the same external account.

6. The `sbt_mint` function must be called by the account which is a subject of the Claim. In other words, predecessor account ID must equal to `Claim.claimer`. The function will:

   - checks if the predecessor is a root account or an implicit account
   - deserializes Claim
   - validates signature
   - validates that the external identity was not used
   - checks if there is enough deposit required to cover minting storage cost
   - request the IAH registry to mint FV SBT and KYC SBT (only when `verified_kyc` is set in the Claim)

## Example Flow

Consider Alice who wants to get Fractal FV SBT.

1. Alice logs in to the Fractal ID system to obtain FV credential (off chain). She is using [i-am-human.app](https://i-am-human.app) (IAH app) to facilitate the whole process.
2. The IAH app will request authentication token from the Fractal ID and pass it to the IAH Verification Oracle to authenticate Alice and check if she has Face Scan credential.
3. IAH Verification Oracle creates and signs a Claim and returns it to the IAH app.
4. IAH app prepares a transaction with the Claim and the Claim signature.
5. Alice sends the signs and sends the transaction to the IAH oracle smart contract.
6. IAH oracle smart contract validates the Claim, and once everything is all right, will mint SBT through the IAH Registry.

NOTE: SBT tokens are minted through a SBT registry - a separate contract responsible for registering SBTs. Basically, the issuer and token registry are separated to provide expected functionality of Soul Bound Tokens.
See the [NEP-393](https://github.com/near/NEPs/pull/393) standard for more details.

## Decentralization

The smart contract can be easily extended to assure Fractal Oracle decentralization

- select multiple, verified parties who will provide oracle service. Each oracle has to provide a security stake.
