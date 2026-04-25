# rompom

CLI tool that turns ROMs into installable packages for Batocera/EmulationStation systems.

## Why package your ROMs?

Most retrogaming setups treat ROMs as loose files — copied somewhere on disk, scraped once,
and forgotten. It works, until you reinstall your system, switch to a new machine, or want to
share your setup with someone else. At that point, you're back to square one.

rompom takes a different approach: it turns each ROM into a proper package, built with
`makepkg` and installable via `pacman`. This means your entire ROM library can be managed the
same way you manage software:

- **Reproducibility** — after a fresh Batocera install, restore your exact library by
  reinstalling your packages. ROMs, metadata, and media assets all come back as they were.
- **Cherry-pick** — install only the games you want. Uninstall cleanly, without leaving
  orphaned files behind.
- **Versioned artifacts** — each package is a PKGBUILD: auditable, storable in git, shareable.
- **Curated sets** — create virtual packages that have no content of their own but declare a
  list of ROM packages as dependencies. A `set-castlevania` package pulls every Castlevania
  game across all systems in one command. A `set-adventure-snes` installs your curated SNES
  selection at once.
- **Automation** — run rompom in CI to keep your repository up to date whenever a ROM changes
  upstream.

## What rompom does

Given a system name, rompom:

1. **Collects ROMs** from an Internet Archive item, filtering by glob patterns, with SHA1
   verification
2. **Queries ScreenScraper** for each ROM to retrieve game metadata: name, description, genre,
   rating, release date, and all available media assets
3. **Generates per ROM:**
   - a `PKGBUILD` ready to build with `makepkg`
   - a `description.xml` in EmulationStation format
4. **Downloads** the ROM and all media assets (video, image, thumbnail, screenshot, bezel,
   marquee, wheel, manual) — skipping files already present and valid, re-downloading only
   what changed

The result is a directory of ready-to-build packages that feed into a `pacman` repository.

## Compared to alternatives

Tools like **Skraper** or Batocera's built-in scraper do one thing well: they scrape metadata
and media assets for ROMs already present on your system. They are polished, easy to use, and
cover a wide range of systems. If you already have your ROMs and just want a populated
gamelist, they are the right tool.

rompom addresses a different set of problems:

- **ROM acquisition** — Skraper and Batocera's scraper assume you already have the files.
  rompom fetches them from Internet Archive and verifies each one against its SHA1 checksum.
  No manual searching or downloading.
- **Reproducibility** — scraping produces loose files scattered across your filesystem. If you
  reinstall Batocera, you scrape again from scratch. With rompom, you reinstall your packages
  and everything — ROMs, metadata, media — is back exactly as it was.
- **Curated sets** — a scraper has no concept of grouping. With rompom, a virtual package can
  declare a set of ROM packages as dependencies, giving you one-command installs for
  hand-picked collections.
- **Auditability** — a PKGBUILD is a text file you can read, diff, store in git, and share.
  A scraped gamelist is an opaque snapshot.

**Honest limitations of rompom:** it requires a ScreenScraper developer account, has no GUI,
and the value only becomes clear once you manage a repository or multiple machines. For a
single casual setup, Skraper is simpler.

## Prerequisites

- A **ScreenScraper developer account** — register at [screenscraper.fr](https://www.screenscraper.fr).
  Both a user account and a developer account are required. The developer account unlocks
  concurrent API threads, which rompom uses to process ROMs in parallel.
- **makepkg** — to build the generated PKGBUILDs. Available natively on Arch-based systems,
  or via [rom-builder](https://github.com/gfriloux/rom-builder), a Docker image that provides
  a ready-to-use build environment.
- A **HTTP server** to host your package repository (nginx, Caddy, or even
  `python -m http.server` for local use).
- A **Batocera** installation to install and use the packages.

## Installation

### NixOS / Home Manager

Add rompom to your flake inputs and enable the Home Manager module:

```nix
inputs.rompom.url = "github:gfriloux/rompom";
```

```nix
{ inputs, ... }: {
  imports = [ inputs.rompom.homeManagerModules.rompom ];

  config.rompom.rompom.enable = true;
}
```

### Build from source

```
cargo build --release
```

The binary will be at `target/release/rompom`.

> **Note:** pre-built static binaries are planned for future releases.

## Configuration

Copy the sample configuration file to `~/.config/rompom.yml`:

```
cp rompom.yml ~/.config/rompom.yml
```

Then edit it to fill in your ScreenScraper credentials and define your systems.

### ScreenScraper credentials

```yaml
screenscraper:
  dev:
    login: your-dev-login
    password: your-dev-password
  user:
    login: your-user-login
    password: your-user-password

lang:
  - fr
  - en
```

`lang` defines the priority order for descriptions and genre names. Supported values:
`de`, `en`, `es`, `fr`, `it`, `pt`.

### Systems

Each system entry follows this structure:

```yaml
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
```

| Field      | Description                                                        |
|------------|--------------------------------------------------------------------|
| `name`     | Identifier used with `-s`                                          |
| `id`       | ScreenScraper system ID                                            |
| `basename` | Prefix for the generated package name                              |
| `dir`      | ROM directory name on the Batocera filesystem                      |
| `depends`  | Optional Batocera package dependency (e.g. a BIOS package)         |
| `source`   | ROM source: `internet_archive` or `folder`                         |

To find a system's ScreenScraper ID, search for it on
[screenscraper.fr](https://www.screenscraper.fr).

### Sources

**Internet Archive:**

```yaml
source:
  internet_archive:
    - item: atomiswave_complete
      filter:
        - "*.zip"
```

Multiple items can be listed. `filter` is a list of case-sensitive glob patterns applied to
filenames.

**Local folder:**

```yaml
source:
  folder:
    path: /path/to/roms
    filter:
      - "*.zip"
      - "*.sfc"
```

### Migrating an existing config

If you are upgrading from an older version of rompom, run:

```
rompom --update-config
```

This detects and applies any required migrations (missing `lang` field, old `ia_items`
format).

## First run

```
rompom -s atomiswave
```

rompom opens a terminal UI split into three panels:

- **Discovery** — ROM identification in progress: querying ScreenScraper, generating PKGBUILDs
- **Downloads** — ROM and media asset downloads
- **Completed** — finished ROMs, with per-media icons showing what was downloaded, already
  up-to-date, or unavailable

### Unidentified ROMs

When a ROM is not found automatically on ScreenScraper, rompom pauses on that ROM and opens
an identification modal. It presents a list of candidates from a name-based search — navigate
with the arrow keys and press Enter to confirm. If none match, press `i` to enter a
ScreenScraper game ID manually.

Other ROMs continue processing in parallel while the modal is open.

### Interrupting a run

Press `Ctrl-C` to interrupt. rompom saves the current progress to `<system>.run.yml`. On the
next run, you will be offered to resume from where it stopped — only pending ROMs are
reprocessed, completed ones are skipped.

## Building and deploying packages

Once rompom finishes, each ROM has its own directory containing a `PKGBUILD`, a
`description.xml`, and all media assets.

### Building

With `makepkg` directly:

```
cd atomiswave/dolphin
makepkg
```

Or with [rom-builder](https://github.com/gfriloux/rom-builder), a Docker image that provides
a ready-to-use build environment without requiring a local Arch setup:

```
cd atomiswave/dolphin
docker run -v "$PWD":"/code" rom-builder
```

Both produce `.pkg.tar.zst` files.

### Creating a repository

Use `repo-add` to create a `pacman`-compatible repository database:

```
repo-add roms.db.tar.gz atomiswave/*.pkg.tar.zst
```

Serve the directory over HTTP — any static file server works.

### Installing on Batocera

Add your repository to `/etc/pacman.conf` on Batocera:

```ini
[roms]
Server = http://your-server/roms
```

Then install packages with `pacman`:

```
pacman -S atomiswave-rom-dolphin
```

### Curated sets

Create a minimal `PKGBUILD` with no sources and a `depends` list to define a set:

```bash
pkgname=set-castlevania
pkgver=1
pkgrel=1
depends=(
  'nes-rom-castlevania'
  'snes-rom-super-castlevania-iv'
  'megadrive-rom-castlevania-bloodlines'
)

package() { true; }
```

Installing `set-castlevania` pulls all listed games in one command.

## Limitations & known issues

- **ScreenScraper dependency** — if the service is unavailable or throttled, rompom waits.
  Throughput depends on your developer account tier.
- **Internet Archive dependency** — if an IA item is taken down or renamed, the source stops
  working. There is no automatic fallback.
- **Unrecognized ROMs** — some ROMs are simply not in the ScreenScraper database. The
  identification modal allows manual matching, but it requires human input for each one.
- **Multi-file systems** — single-file ROMs (Master System, NES, SNES, Mega Drive…) work out
  of the box. Systems with more complex file layouts may require a dedicated PKGBUILD
  template.

## Contributing / Development

### Dev environment

```
nix develop
```

This drops you into a shell with cargo, rustc, clippy, rustfmt, and rust-analyzer. On first
entry, the pre-commit git hook is installed automatically. It runs on every commit to enforce
formatting and linting.

To run checks manually:

```
nix flake check
```

This validates Nix formatting (alejandra), dead Nix code (deadnix), Nix linting (statix),
and Rust formatting (rustfmt).

### Working against local library changes

`screenscraper` and `internetarchive` are pinned by git tag in `Cargo.toml`. To work against
local checkouts, comment the `tag =` line and uncomment the `path =` line for the relevant
dependency:

```toml
screenscraper = {
  git = "https://github.com/gfriloux/screenscraper",
  # tag = "v0.x.y",
  path = "../screenscraper",
}
```

Restore the `tag =` line before tagging a new release.

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for the full version history.
