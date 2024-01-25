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

### Breaking Changes

### Bug Fixes

## v1.2.0 (2024-01-25)

### Breaking Changes

- `class_metadata` has been renamed to `sbt_class_metadata` to unify the SBT Issuer interface.

## v1.1.0 (2023-12-20)

### Features

- `admin_mint` to allow manual verification and manual mint by the admin / dao.
- `get_admins` query.
- Added `ClassMetadata` to the Oracle contract and the `class_metadata` query.

## Improvements

- Decrease `sbt_mint` recommended storage deposit.

## v1.0.1 (2023-08-17)

## v1.0.0 (2023-05-20)
