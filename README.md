# rompom

CLI tool that automates ROM packaging for Batocera/EmulationStation systems.

Given a system name, rompom collects ROMs (from Internet Archive or a local folder), enriches
them with metadata from ScreenScraper, and produces ready-to-build packages.

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
- For each system, a `source` block (Internet Archive or local folder)

```yaml
screenscraper:
  dev:
    login: mydevlogin
    password: mydevpassword
  user:
    login: myuserlogin
    password: myuserpassword

lang:
  - fr
  - en

systems:
  - name: atomiswave
    id: 53
    basename: atomiswave-rom-
    depends: bios-atomiswave
    dir: atomiswave
    source:
      internet_archive:
        - item: atomiswave_complete
          filter:
            - "*.zip"

  - name: snes
    id: 4
    basename: snes-rom-
    dir: snes
    source:
      folder:
        path: /path/to/local/snes/roms
        filter:
          - "*.zip"
          - "*.sfc"
```

Each system entry:

| Field      | Description                                             |
|------------|---------------------------------------------------------|
| `name`     | Identifier used with `-s`                               |
| `id`       | ScreenScraper system ID                                 |
| `basename` | Prefix for the generated package name                   |
| `depends`  | Optional Batocera package dependency (e.g. a BIOS)      |
| `dir`      | ROM directory name on the Batocera filesystem           |
| `source`   | ROM source: `internet_archive` (list of IA items) or `folder` (local path) |

`filter` is a list of glob patterns applied to filenames (case-sensitive).

## Dependencies

- [screenscraper](https://github.com/gfriloux/screenscraper) — ScreenScraper.fr API + media download
- [internetarchive](https://github.com/gfriloux/internetarchive) — archive.org metadata + download

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for the full version history.
