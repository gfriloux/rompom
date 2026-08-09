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
