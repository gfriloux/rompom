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
  **Your account tier sets the pace of the whole run**: ScreenScraper reports a
  `maxthreads` value, and rompom sizes its ScreenScraper semaphore from it. A tier
  allowing one thread means one lookup at a time, whatever the machine — downloads and
  packaging still run in parallel around it, but identification is the bottleneck.
- A **Nerd Font** in your terminal — the media columns are Nerd Font glyphs and render as
  identical empty boxes without one. `--ascii` replaces them with letters if you would
  rather not install a font.
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

### Command-line flags

| flag | what it does |
|---|---|
| `-s`, `--system SYSTEM` | the system to scrape — the name must match `rompom.yml` exactly |
| `--init` | write a starter `rompom.yml` — refuses if one is already there |
| `--list-systems` | list the systems declared in `rompom.yml` with their id and source, then exit |
| `--update-config` | interactive migration of an outdated `rompom.yml` |
| `--plain` | one line per finished ROM instead of the full-screen interface — implied when stdout is not a terminal |
| `--resume yes\|no` | answer the interrupted-run prompt up front instead of being asked |
| `--ascii` | replace the Nerd Font media icons with ASCII letters |
| `--debug` | write `<system>.debug.log` with the per-ROM pipeline decisions |
| `-h`, `--help` | usage |
| `-V`, `--version` | version, and nothing else — works without a config file |

`--list-systems` is the answer to *"rompom says my system is unknown"*: it prints exactly
the names that are accepted, and marks the systems that have no `source` block and
therefore cannot be run.

### Non-interactive runs

When stdout is not a terminal — a pipe, a file, a CI job — rompom drops the full-screen
interface on its own and writes one line per finished ROM instead:

```
[12/340] ✓ Sonic The Hedgehog  󰗚 󰕧 󰋩 󰋫 󰹙
[13/340] = Streets of Rage 2
[14/340] ✗ Some Unknown Game  not identified — needs manual identification
```

`--plain` forces the same thing inside a real terminal. The end-of-run summary is printed
either way. A run with no terminal at all needs nothing else:

```
rompom -s snes --plain --resume no < /dev/null
```

### The interface

In a terminal, rompom opens a full-screen grid: **one row per ROM, in the order it was
collected, for the whole run**. A ROM never moves — columns say how far it got.

A banner sits above it: how far along the run is, how many ROMs are new / unchanged /
failed / waiting to be identified, and a throughput line — ROMs per minute, MiB/s, the
ETA, and how many workers are busy. The rates are read over **the last minute**, not over
the whole run, so they still react when the network slows down. The ETA reads `—` when
nothing has finished recently, rather than showing a number that is no longer true.

```
#     rom                        id   pkg  rom   󰗚  󰕧  󰋩  󰋫  󰹙  󱂬  󰯃  󰊢  󰂺  time    status
1198  Yoshi's Island             ✓    ✓    ✓     ●  ●  ●  ●  ●  ●  ●  ●  ○  4.6s    done
1199  Bahamut Lagoon (J)         ✓    ✓    ✗     ·  ·  ·  ·  ·  ·  ·  ·  ·  18.2s   sha1 mismatch
1201  Kirby Super Star           =    =    =     ●  ●  ●  ●  ●  ●  ●  ●  ○  0.2s    unchanged
1206  Super Mario RPG            ⠹    ·    ·     ·  ·  ·  ·  ·  ·  ·  ·  ·  1.2s    identifying
1208  Zelda: Link to the Past    ·    ·    ·     ·  ·  ·  ·  ·  ·  ·  ·  ·  —       queued
```

- **`id` / `pkg` / `rom`** — identification, PKGBUILD, ROM transfer. `·` not reached,
  spinner running, `✓` done, `=` nothing to do, `✗` failed.
- **The nine dots** — one per tracked asset, in the order of the header icons
  (description, video, image, thumbnail, screenshot, bezel, marquee, wheel, manual).
  `●` green fetched now, `●` gray already up to date, `○` red not on ScreenScraper,
  `◐` in progress, `·` not tried.

The window scrolls itself to keep the working area in view. The footer says how many ROMs
are above and below it.

| key | effect |
|---|---|
| `↑` `↓` | move the cursor; the selected ROM unfolds three detail lines below it |
| `g` / `G` | jump to the top / back to the bottom (`G` also re-enables auto-scrolling) |
| `Ctrl-C` | interrupt, saving `<system>.run.yml` |

Moving the cursor stops the automatic scrolling — a list sliding under the cursor cannot
be read. `G` gives it back.

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

`--resume yes` or `--resume no` answers that prompt up front. With stdin closed and no
flag, rompom does **not** resume: it deletes `<system>.run.yml` and starts a fresh run.
Everything expensive is skip-if-valid, so a fresh run re-checks rather than re-does.

### Exit codes

| code | meaning |
|---|---|
| `0` | the run finished — individual ROMs may still have failed, see the `Failures` section of the summary |
| `1` | rompom could not run: no config directory, unreadable or invalid `rompom.yml`, unknown system, a system with no `source` block, ScreenScraper refusing the credentials |
| `2` | the command line was wrong: unknown flag, missing value, no `-s` |

A mistyped system name used to exit `0`, which in CI is indistinguishable from a run that
scraped a whole library.

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

### Files rompom writes

All three land in the **current working directory**, not next to the ROMs and not under
`$XDG_STATE_HOME` — so running rompom from two different directories gives two
independent histories of the same system.

| file | what it is |
|---|---|
| `<system>.state.yml` | what the last run found: ScreenScraper game ids, ROM and media sha1s. Rewritten every 30 s and once at the end. |
| `<system>.run.yml` | only while a run is interrupted — the per-ROM step statuses the resume prompt reads. Deleted when the run completes or when you decline to resume. |
| `<system>.debug.log` | only with `--debug`. Truncated at the start of each run. |

**Deleting `state.yml` is not free.** It is the only record of what has already been done:
without it every ROM looks new, so every ROM and every media asset is downloaded again,
every `description.xml` is rewritten, and every `pkgver` is bumped — which republishes the
entire library to anyone tracking your repository. Move it aside rather than delete it if
you are only trying something out.

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

This drops you into a shell with cargo, rustc, clippy, rustfmt, rust-analyzer, just,
git-cliff and cargo-audit. On first entry, the pre-commit git hook is installed
automatically. It runs on every commit to enforce formatting and linting.

### Quality gates

The `Justfile` is the single definition of the gates — pre-commit, the CI workflow and
local development all call the same recipes:

```
just ci             # version-check + fmt-check + lint + test
just fmt            # rustfmt --config tab_spaces=2, in place
just audit          # CVE scan of the dependency tree
```

`just version-check` guards against the version drifting between `Cargo.toml`, `Cargo.lock`
and `packages/rompom/default.nix`.

Nix-side checks run separately:

```
nix flake check
```

This validates Nix formatting (alejandra), dead Nix code (deadnix), Nix linting (statix),
and Rust formatting (rustfmt).

### Contributing

Read [`PROCEDURE_PLANS.md`](PROCEDURE_PLANS.md) before starting: it defines the planning
procedure, the commit convention and the test discipline. In short — dedicated branch,
atomic [Conventional Commits](https://www.conventionalcommits.org/) with a real scope
(never `all`), docs in the same commit as the code.

Commit subjects matter: `CHANGELOG.md` is generated from them, so each subject becomes a
release note verbatim.

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

[CHANGELOG.md](CHANGELOG.md) holds the version history. It is **generated** from the
Conventional Commits by [git-cliff](https://git-cliff.org) — run `just changelog` rather
than editing it.

Entries for **v0.15.0 and earlier** predate git-cliff and are kept, in their original and
much more detailed form, in [CHANGELOG-legacy.md](CHANGELOG-legacy.md).

## Release

Versioning follows [SemVer](https://semver.org/). The tag is set by the **maintainer** and
triggers publication.

1. `just release X.Y.Z` — bumps the version in `Cargo.toml`, `Cargo.lock` and
   `packages/rompom/default.nix`, then regenerates `CHANGELOG.md` for that version.
2. Review the diff, commit as `chore(release): vX.Y.Z`, merge onto `master`.
3. Tag and push:

   ```
   git tag -a vX.Y.Z -m "vX.Y.Z" && git push origin vX.Y.Z
   ```

Pushing the tag runs [`.github/workflows/release.yml`](.github/workflows/release.yml), which
refuses to publish if the tag does not match the version in `Cargo.toml`, then creates a
GitHub release whose body is the `CHANGELOG.md` entry for that exact version, with the
static musl binary attached.

Dependencies (Cargo, flake inputs, GitHub Actions) are kept up to date by **Renovate**.
