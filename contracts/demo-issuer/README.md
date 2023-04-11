# Demo SBT Issuer

Minimum implementation of SBT.
NEP: https://github.com/near/NEPs/pull/393

Functions:

- `sbt_mint(receiver: AccountId, memo?: string)` -- requests registry to mint a new SBT. Only admin can call it.
- `add_admin(account: AccountId, memo?: string)` -- registers new admin. Any admin can add new admins.

To query SBTs, you have to query the registry contract directly.
