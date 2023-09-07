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

- Added `sbt_update_token_references` into the registry and SBT trait.
- Added `token_reference` event (`Nep393Event::TokenReference`).

### Breaking Changes

- Recommended `cost.mint_deposit` is decreased by 0.001 miliNEAR (in total).
- `soul_transfer` conflict resolution is updated to panic.

### Bug Fixes

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
