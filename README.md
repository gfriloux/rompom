# rompom

CLI tool that automates ROM packaging for Batocera/EmulationStation systems.

Given a system name, rompom fetches ROM lists from Internet Archive, enriches them with
metadata from ScreenScraper, and produces ready-to-build packages.

## What it does

rompom runs in 3 phases:

1. **Discovery** — queries ScreenScraper for each ROM to retrieve game metadata
   (name, description, genre, rating, region, release date, media assets)
2. **Packaging** — generates per ROM:
   - a `PKGBUILD` (Arch/Batocera package format)
   - a `description.xml` (EmulationStation format)
3. **Downloads** — fetches the ROM and all available media assets with SHA1 verification:
   - skips files that are already present and valid
   - re-downloads files with a checksum mismatch
   - media: video, image, thumbnail, bezel, marquee, screenshot, wheel, manual

The generated `PKGBUILD` can then be built with `makepkg` (or a Docker image wrapping it)
to produce a `.pkg.tar.zst` installable on Batocera via its package manager.

## Usage

```
rompom -s <system>
```

Example:

```
rompom -s atomiswave
```

## Configuration

Copy `rompom.yml` from the repo root to `~/.config/rompom.yml` and fill in:

- ScreenScraper credentials (`screenscraper.dev` and `screenscraper.user`)
- For each system, an `ia_items` list pointing to Internet Archive item identifiers

```yaml
screenscraper:
  dev:
    login: mydevlogin
    password: mydevpassword
  user:
    login: myuserlogin
    password: myuserpassword

systems:
  - name: atomiswave
    id: 53
    basename: atomiswave-rom-
    depends: bios-atomiswave
    dir: atomiswave
    ia_items:
      - item: atomiswave_complete
        filter: "*.zip"
```

Each system entry:

| Field      | Description                                             |
|------------|---------------------------------------------------------|
| `name`     | Identifier used with `-s`                               |
| `id`       | ScreenScraper system ID                                 |
| `basename` | Prefix for the generated package name                   |
| `depends`  | Optional Batocera package dependency (e.g. a BIOS)      |
| `dir`      | ROM directory name on the Batocera filesystem           |
| `ia_items` | List of Internet Archive items + glob filter            |

## Dependencies

- [screenscraper](https://github.com/gfriloux/screenscraper) — ScreenScraper.fr API + media download
- [internetarchive](https://github.com/gfriloux/internetarchive) — archive.org metadata + download

## Changelog

### v0.6.0

- **Terminal UI** — progress is now displayed with spinners, one per ROM.
  Download states are explicit: checking, downloading, already present, checksum mismatch.
- **Media downloads** — rompom now downloads all available media assets alongside the ROM:
  video, image, thumbnail, bezel, marquee, screenshot, wheel, manual.
  Files already present and valid are skipped automatically.
- **Dependency updates** — `screenscraper` v0.4.0, `internet_archive` v0.2.0.

### v0.5.0

- Download ROM files from Internet Archive with SHA1 verification
- Skip already-downloaded ROMs

### v0.4.x and earlier

Initial releases — PKGBUILD and description.xml generation only.
