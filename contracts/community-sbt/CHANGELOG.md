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

# CHANGELOG: Community SBT

## Unreleased

### Features

### Breaking Changes

### Bug Fixes

## v4.3.0 (2023-09-07)

### Features

- Added support for multiple admins authorized to manage issuers.

### Breaking Changes

- Contract field type for `admin` changed from `AccoundId` -> `LazyOption<Vec<AccountId>>`
- Recommended `cost.mint_deposit` is decreased by 0.001 miliNEAR (in total).

## v4.2.0 (2023-08-25)

The release introduces more class level customizations.

### Features

- Admin can set `max_ttl` per class rather than per contract.
- Allow minters to revoke tokens (previously, only contract admin DAO could do that).
- Allows minters to define class level metadata. So, now you can set meatadata for the whole issuer (contract), class and for every token. Class common stuff (like icons) should go on the class metadata level.
- New sbt_mint_many function allowing minter DAO to mint many tokens at once.

### Deployments

- `community.i-am-human.near`: tx: `6NEz1NASdExWF5wRzwiAHoHkCxWctgK96rJPGFHgoQz5`, migration: `Z8cH2vFT229av5z28xpFiX8wPZcx6N1UZs3fyFfwPLy`
