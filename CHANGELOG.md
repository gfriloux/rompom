# Changelog

All notable changes to rompom. Format inspired by [Keep a Changelog], versions follow
[SemVer]. Generated from [Conventional Commits] by git-cliff — do not edit by hand,
run `just changelog` instead.

Entries for **v0.15.0 and earlier** were written by hand before this repository adopted
git-cliff; they are preserved verbatim, with far more detail, in
[`CHANGELOG-legacy.md`](./CHANGELOG-legacy.md).

[Keep a Changelog]: https://keepachangelog.com/en/1.1.0/
[SemVer]: https://semver.org/
[Conventional Commits]: https://www.conventionalcommits.org/

## [0.17.0] — 2026-08-10

### Features

- **ui**: Show why a ROM failed, in the panel and in the summary ([`9366433`](https://github.com/gfriloux/rompom/commit/93664337517275dbbc5d15e96f99c75107bfbfc4))
- **modal**: Tell a wrong game ID from a network failure ([`4e0558a`](https://github.com/gfriloux/rompom/commit/4e0558ad20f1c4b79036e0c9d04ed09f39d4545c))

### Bug fixes

- **state**: Survive a crash without losing the run's state ([`b0a2571`](https://github.com/gfriloux/rompom/commit/b0a25712af25e2cf96364bc3550badd0d35eaf85))
- **package**: Write the OpenBOR launcher into the ROM directory ([`96e6130`](https://github.com/gfriloux/rompom/commit/96e6130ae898a3cc893107c1df23beb8db1964e4))
- **conf**: Name the file and the line when the configuration is unreadable ([`f8b0bfe`](https://github.com/gfriloux/rompom/commit/f8b0bfe2e5a6472e14bfba01d03ba898d6874ddb))
- **main**: Stop printing ScreenScraper credentials on a failed connection ([`f27ab41`](https://github.com/gfriloux/rompom/commit/f27ab414bae9100f36bd13d943baf49b4beb040e))
- **discovery**: Stop treating a ScreenScraper outage as an unknown game ([`18280c2`](https://github.com/gfriloux/rompom/commit/18280c26f37e9fee99a46d9e0dbb24fef267b2cf))

### Refactoring

- **worker**: Type step failures instead of string sentinels ([`fb5683a`](https://github.com/gfriloux/rompom/commit/fb5683a329e64adefa90dcbb1238f0fb82c8bd48))

### Miscellaneous

- **plans**: Record the v0.17.0 checks that were actually run ([`fe7c8cd`](https://github.com/gfriloux/rompom/commit/fe7c8cd6f9fb334e508a2a64854a045a860ea490))
- **plans**: Record what P1.1 turned up ([`e66e6b9`](https://github.com/gfriloux/rompom/commit/e66e6b9164e3c2a5d37bf988b6f4ee01b82db8d8))
- **plans**: Retire the root plans and open the v0.17.0 plan ([`bb1f64c`](https://github.com/gfriloux/rompom/commit/bb1f64c34a43778dab9903bbd2076efb0a47c0dc))

### Dependencies

- **deps**: Bump screenscraper to v0.7.0 ([`b925e01`](https://github.com/gfriloux/rompom/commit/b925e011f7e73ae563892b95364a396e16f9a21d))

## [0.16.0] — 2026-08-09

### Bug fixes

- **package**: Fall back to the epoch on a malformed ScreenScraper date ([`72f6d5e`](https://github.com/gfriloux/rompom/commit/72f6d5e0e26d8a96c8511ecba6de4a5c381890a0))
- **collect**: Report collection failures instead of panicking under the TUI ([`aa5853a`](https://github.com/gfriloux/rompom/commit/aa5853a57401cb3f8abf98f884803eef9f0b0d3e))
- **state**: Restart an unfinished ROM instead of resuming it mid-pipeline ([`2b4010d`](https://github.com/gfriloux/rompom/commit/2b4010d76d65d1a466d81e7129f4952ea23ebe0d))
- **pipeline**: Stop dispatching successors after a definitive failure ([`fa55cbf`](https://github.com/gfriloux/rompom/commit/fa55cbfe7fd80713c099feb5ac818c962801bb36))
- **pipeline**: Turn a handler panic into a failed step instead of a deadlock ([`2449c4a`](https://github.com/gfriloux/rompom/commit/2449c4ae890158e80fb347afaaf28c74c117cde7))
- **pipeline**: Reject path separators in media filenames ([`ea649a7`](https://github.com/gfriloux/rompom/commit/ea649a7140e85f2f459d5530ff666cce9c6bd573))
- **package**: Escape every ScreenScraper field injected into a PKGBUILD ([`8aa9ff2`](https://github.com/gfriloux/rompom/commit/8aa9ff21e663f895aef28286da43edd0606e2c6e))
- **deps**: Upgrade quick-xml to 0.41 (RUSTSEC-2026-0194/0195) ([`dcfa1d6`](https://github.com/gfriloux/rompom/commit/dcfa1d693ead1673a26498b52a5412140b96b82c))
- **nix**: Align the rompom package version with Cargo.toml ([`b907097`](https://github.com/gfriloux/rompom/commit/b907097e9f58408df2591d1b4e4fab6fdc7143b7))

### Documentation

- Close the P0 lot and record what the diagnosis missed ([`34de4cb`](https://github.com/gfriloux/rompom/commit/34de4cb3b2748621ab7e12863aaba7e45c0cb7b9))
- Record the whitelist arbitration in the v0.16.0 plan ([`833ce5e`](https://github.com/gfriloux/rompom/commit/833ce5e594275c0b47661b245945abf139b9e2f0))
- Land a regression test with its fix instead of as a red commit ([`812b427`](https://github.com/gfriloux/rompom/commit/812b4279b0075410cfeb24e29d7187cfba0cd029))
- Add the v0.16.0 plan for the failure-path lot ([`2a4515d`](https://github.com/gfriloux/rompom/commit/2a4515d3063b81b210333b9f5a733555bb44a8b4))
- Record the dependency-security debt in the roadmap ([`509d935`](https://github.com/gfriloux/rompom/commit/509d935d27f4407c68824cbd535e8ca7fb18c2b3))
- Add PROCEDURE_PLANS.md defining the working norms ([`eb5ac98`](https://github.com/gfriloux/rompom/commit/eb5ac9874cbf445bbb5312258dea885283aabd4a))

### Tests

- **package**: Pin the description.xml output with a snapshot ([`0576bc2`](https://github.com/gfriloux/rompom/commit/0576bc20c439e6eedbb9c32cdffec8e548c92984))

### Continuous integration

- Allowlist the advisories that cannot be fixed from this repository ([`5028584`](https://github.com/gfriloux/rompom/commit/5028584206ebdd906849cc74d5523e5091e286f9))
- Enable Renovate for dependency updates ([`a61c2ce`](https://github.com/gfriloux/rompom/commit/a61c2ce4f3b4f88a8489aa3b2f412d9c91a7f773))
- Add GitHub Actions workflows for CI and release ([`d39a4ce`](https://github.com/gfriloux/rompom/commit/d39a4ce5530f557d62d95947c38e2dc643acc3ba))
- Generate CHANGELOG.md from the Conventional Commits ([`562ad29`](https://github.com/gfriloux/rompom/commit/562ad297da5a411762d9653b7120f4b2b45c8316))
- Add a Justfile as the single definition of the quality gates ([`cf75a23`](https://github.com/gfriloux/rompom/commit/cf75a23a432f5756819bac6cce25bc984cfc6cd1))

### Miscellaneous

- **nix**: Add git-cliff and cargo-audit to the dev shell ([`cfac737`](https://github.com/gfriloux/rompom/commit/cfac73775309b064e093cb0d76ec876e5f6a82fb))

### Dependencies

- **deps**: Update Rust crate openssl to v0.10.80 [SECURITY] (#19) ([`fa9c379`](https://github.com/gfriloux/rompom/commit/fa9c3798c8d56800b8dfdd06288921752cb76856))
- **deps**: Update Rust crate chrono to v0.4.45 (#20) ([`63988d6`](https://github.com/gfriloux/rompom/commit/63988d669e4a74ae0b92193848b94d0bda1c5284))

## [0.15.0] — 2026-04-26

### Features

- **all**: Add support for multi-disk systems ([`af89fe9`](https://github.com/gfriloux/rompom/commit/af89fe987dc61dc067194e862b00b66566cd8590))

### Documentation

- **all**: Rework readme ([`08d24ca`](https://github.com/gfriloux/rompom/commit/08d24cabfca3945d755d40dc2b714d3c8d6176e4))

## [0.14.2] — 2026-04-25

### Bug fixes

- **all**: Fix cancel/resume of rompom ([`efd7300`](https://github.com/gfriloux/rompom/commit/efd7300ec8d1df1f51e313e3081008299de8f31d))

## [0.14.1] — 2026-04-25

### Refactoring

- **all**: Split large modules into focused sub-modules ([`8733b56`](https://github.com/gfriloux/rompom/commit/8733b560bdeda8f32d2fb6114e648c2eceedeb4b))

### Dependencies

- **all**: Update Cargo.lock ([`7e8af35`](https://github.com/gfriloux/rompom/commit/7e8af35f5b27de24e5345a7d0870b4d2bcc386ea))

## [0.14.0] — 2026-04-25

### Features

- **ui**: Media coverage should count unchanged medias ([`d45eb74`](https://github.com/gfriloux/rompom/commit/d45eb74675e68dce426f9809b22dbf538ea3bdcc))
- **ui**: Track description.xml changes with icon and pkgver bump ([`12aec61`](https://github.com/gfriloux/rompom/commit/12aec61fc7605419953abbfb4812fa575fadaa6b))
- **all**: Add --debug flag to log per-ROM pipeline decisions ([`e364ea7`](https://github.com/gfriloux/rompom/commit/e364ea79d94c4f3606d5429598ef904bc688990b))
- **ui**: Distinguish downloaded vs unchanged media icons ([`00bbd9d`](https://github.com/gfriloux/rompom/commit/00bbd9d57c6727dfa50fab02e3f5da90a4c0635e))
- **all**: Refactor pipeline to DAG-based task queue with state machines (#18) ([`0564682`](https://github.com/gfriloux/rompom/commit/05646820f7bbde9284dc314e9ab74c9c06cabaab))
- **all**: Add package + home-manager module ([`00cc220`](https://github.com/gfriloux/rompom/commit/00cc2207b00185df3a494fd1c869ab113a45db67))

### Bug fixes

- **all**: Strip ROM extension from normalized pkgname ([`b53acc6`](https://github.com/gfriloux/rompom/commit/b53acc6f6bbd5436a1873e8c5b6e9747c198e243))

### Miscellaneous

- **nix**: Set rompom as default package ([`c0ea50a`](https://github.com/gfriloux/rompom/commit/c0ea50ae55093e1fcbd131aff973ef89fc7c9d2a))

### Dependencies

- **all**: Update Cargo.lock ([`8c223d1`](https://github.com/gfriloux/rompom/commit/8c223d1e3f8e9261f220a5a65f1834474bad7353))
- **all**: Cargo update ([`a03e13e`](https://github.com/gfriloux/rompom/commit/a03e13ebef431e32f5cc1a53821427ffd83ba944))

## [0.13.0] — 2026-04-19

### Features

- **all**: Feat: skip SHA1 computation using mtime+size fast-path for folder ROMs ([`c4fec03`](https://github.com/gfriloux/rompom/commit/c4fec03317cb75c135ca2811860e1e60bf025ed8))
- **all**: Add incremental update support with state tracking ([`a5630d5`](https://github.com/gfriloux/rompom/commit/a5630d5d10bd70ceec38b66f87bf9f19f3e5e8a7))

## [0.12.0] — 2026-04-19

### Features

- **all**: Allows to search for a game or identify ROM by it's SS ID ([`374b0b0`](https://github.com/gfriloux/rompom/commit/374b0b001499f8452827fc9e17619c1408418fe2))

## [0.11.0] — 2026-04-19

### Features

- **all**: Work with folders too ([`235b6c0`](https://github.com/gfriloux/rompom/commit/235b6c0756e1be5384d81560386df2a1585c83e8))
- **all**: Change ROM sources, config needs to be updated ([`2e6cedf`](https://github.com/gfriloux/rompom/commit/2e6cedfc07448037554fbf28db56e289f5f04363))

### Documentation

- **all**: Update for v0.11.0 ([`1d3070a`](https://github.com/gfriloux/rompom/commit/1d3070a03e5766e9da93daf31e2c8722ffbcfe62))

## [0.10.0] — 2026-04-19

### Features

- **all**: Add bottom line on completed pannel to print media legend ([`8d1e56b`](https://github.com/gfriloux/rompom/commit/8d1e56bc581f6b22f7b0d2ff30efff69b974d740))
- **all**: Use real xml lib to create description.xml ([`54c5702`](https://github.com/gfriloux/rompom/commit/54c5702dbc73a5080023ed1dd42fde482cef1fda))
- **all**: Use template to create OpenBOR launcher ([`9943889`](https://github.com/gfriloux/rompom/commit/9943889520ee6ef2d6de31c89aa7022dbb902581))
- **all**: Whole PKGBUILD is now created from template ([`6d22b55`](https://github.com/gfriloux/rompom/commit/6d22b559e8d8dbb9af3c318d2c868a6b72962cf1))
- **all**: Use templates to make it easier to maintain code ([`d0dc3d2`](https://github.com/gfriloux/rompom/commit/d0dc3d22de929e29c01d4bdacfcdd1e43954fca9))
- **all**: Change how we manage languages ([`8ffd5b4`](https://github.com/gfriloux/rompom/commit/8ffd5b403b4940de615b5755190efb59e75d25fb))

### Refactoring

- **all**: Naming convention ([`38c3f2a`](https://github.com/gfriloux/rompom/commit/38c3f2aed52bee518551d1f7c1d08be3e6a8d5cd))
- **all**: Simplify pkgbuild logic ([`d884126`](https://github.com/gfriloux/rompom/commit/d8841266302a5d83c64861538b5add6ca69c9b07))

### Documentation

- **all**: Update ([`c8cb234`](https://github.com/gfriloux/rompom/commit/c8cb2342784f180dc7ffaa07f6db48ddc926312e))
- **all**: Update changes on v0.10.0 ([`c839ace`](https://github.com/gfriloux/rompom/commit/c839ace44c4a7ef7d6198702db1f9af9067e5b80))

## [0.9.0] — 2026-04-18

### Features

- **all**: Add stats on how scraping went ([`94936b3`](https://github.com/gfriloux/rompom/commit/94936b3935e652a8e061397d5334d7d31f3f98e2))

### Documentation

- **all**: Update ([`0a5e854`](https://github.com/gfriloux/rompom/commit/0a5e854a2e75f792a3bf860618d07ef97f143d1c))

## [0.8.1] — 2026-04-18

### Features

- **all**: Remove the packaging pannel ([`77089ad`](https://github.com/gfriloux/rompom/commit/77089adafaa0c5a8ea6b4ddd14f867ba59b684a3))

### Documentation

- **all**: Update ([`1bb64ea`](https://github.com/gfriloux/rompom/commit/1bb64eac5be87ee16683fedc8ca27b864765d982))

## [0.8.0] — 2026-04-18

### Features

- **ui**: Enhance it. add progress bars ([`945e782`](https://github.com/gfriloux/rompom/commit/945e7825254d60e64eca8c5f6a60ca21dabd6015))
- **all**: Use ratatui for UI ([`4d065f8`](https://github.com/gfriloux/rompom/commit/4d065f8fa3c6be74014b78d2bc88adb0b8fb9603))
- **all**: Run stages in parallel! ([`2ac9a35`](https://github.com/gfriloux/rompom/commit/2ac9a35b0da25e484f9de0a3fd309879757a4d16))

### Documentation

- **all**: Update ([`1636bf8`](https://github.com/gfriloux/rompom/commit/1636bf87d242bafd6d08cab9ce7bf7e9481f9d10))

## [0.6.0] — 2026-04-18

### Features

- **all**: Enhance code inside main.rs ([`a438691`](https://github.com/gfriloux/rompom/commit/a43869107a7e6c2bfe82b67e16c810be57b4d3e9))
- **all**: Rompom will now support downloading of medias ([`3260a91`](https://github.com/gfriloux/rompom/commit/3260a91227c44fa6ee8fb9fc5a4f30ff1e706554))
- **all**: Have an UI! ([`49af833`](https://github.com/gfriloux/rompom/commit/49af833f2a116466801d50e3db87f45f0a2e4cce))
- **all**: Use latest version of screenscraper and internetarchive ([`7d0f224`](https://github.com/gfriloux/rompom/commit/7d0f22430fcb1aa187339f2b3348d83efc4b07a0))

### Bug fixes

- **all**: Don't panic if ia_items doesn't exist ([`23d1aa4`](https://github.com/gfriloux/rompom/commit/23d1aa4e90656072f5847df5b17f8475db122ea9))

### Refactoring

- **all**: Better code overall ([`299c60c`](https://github.com/gfriloux/rompom/commit/299c60cbe830e5c2ce98a48f3f6f9b1bf2074de3))
- **all**: Enhance code in emulationstation.rs ([`d432f98`](https://github.com/gfriloux/rompom/commit/d432f98c309a46df25ce44c0e39a381bd4610acc))
- **all**: Trying to separate things for maintainability ([`d7f30d4`](https://github.com/gfriloux/rompom/commit/d7f30d48788e0af8e48ab7abdfbcee9304428028))

### Documentation

- **all**: First version ([`af79ff2`](https://github.com/gfriloux/rompom/commit/af79ff234ace6aa2dd88c683b8f230378e261cd9))

### Miscellaneous

- **git**: Update ([`4519134`](https://github.com/gfriloux/rompom/commit/4519134ab662295af9e2cda64f1e7292163debbe))
- **git**: Add .gitignore ([`5d061bf`](https://github.com/gfriloux/rompom/commit/5d061bffd4e9052d8deac7ba8b86603ecc9d4a9a))
- **nix**: First version ([`67c91d9`](https://github.com/gfriloux/rompom/commit/67c91d9ba05cf46c43f101c5dad1ad7a8c702410))

## [0.0.2] — 2020-10-01

<!-- generated by git-cliff -->
