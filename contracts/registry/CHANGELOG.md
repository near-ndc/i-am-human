<!-- markdownlint-disable MD013 -->
<!-- markdownlint-disable MD024 -->

<!--
Changelogs are for humans, not machines.
There should be an entry for every single version.
The same types of changes should be grouped.
The latest version comes first.
The release date of each version is displayed.

Usage:

Change log entries are to be added to the Unreleased section. Example entry:

* [#<PR-number>](https://github.com/umee-network/umee/pull/<PR-number>) <description>
-->

# CHANGELOG: Registry

## Unreleased

### Features

- Added `authorized_flaggers` query.
- Added `admin_add_authorized_flagger` method.
- added `is_human_call_lock` method: allows dapp to lock an account for soul transfers and calls a recipient contract when the predecessor has a proof of personhood.

### Breaking Changes

- New contract field: `transfer_lock`.
- `sbt_soul_transfer` will fail if an account has an active transfer lock.

### Bug Fixes

## v1.6.0 (2023-10-08)

### Features

- New `GovBan` flag. Reserved for accounts with a history of misconduct, limiting their governance role while maintaining their voting rights as valued members of the Voting Body.
- `sbt_revoke_by_owner` returns true if the issuer should continue to call the method to revoke all tokens. Otherwise the function return false. Moreover, the method has been improved and optimized.

### Breaking Changes

- `sbt_mint` will set `issue_at` to the current time in milliseconds, if the value was not provided.

## v1.5.0 (2023-09-07)

### Features

- Added `sbt_update_token_references` into the registry and SBT trait.
- Added `token_reference` event (`Nep393Event::TokenReference`).

### Breaking Changes

- Recommended `cost.mint_deposit` is decreased by 0.001 milliNEAR (in total).
- `soul_transfer` conflict resolution is updated to panic.
- Default `registry.sbt_soul_transfer` limit is decreased from 25 to 20.

## v1.4.0 (2023-08-23)

### Features

- Account flagging feature (blacklist / whitelist=verified). Note: account flag is transferred during the `soul_transfer`.
- Moving `registry.is_human_call` method out of experimental.
- New `registry.sbts` method.
- New `registry.sbt_classes` method.
- `ClassSet` and `SBTs` types have been moved to the sbt crate to reuse it in other contract (eg elections)

### Breaking Changes

Required storage deposits have been updated -> see the `cost` crate.

### Deployments

- `registry.i-am-human.near`: tx: `EdWfAeqKaAJ5iUnH1PjizmZRchywTeFQp2shumKsDqoa`, migration: `AhDC8Vku52rv67j8W3CgdRhKwo27ryvbZ3jt8WdRooau`
