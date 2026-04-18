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

### v0.9.0

- **Media icons in Completed log** — each finished ROM now shows a Nerd Font icon per
  media type, colored green if downloaded or red if unavailable:
  `󰕧` video · `󰋩` image · `󰋫` thumbnail · `󰹙` screenshot · `󱂬` bezel · `󰯃` marquee · `󰊢` wheel · `󰂺` manual
- **End-of-run summary** — after the TUI exits, rompom prints a concise report:
  success/error counts and a per-media-type coverage bar with percentages.
  Requires a Nerd Font terminal for icons to render correctly.

**Migration from v0.8.x:** none — configuration file is unchanged.

---

### v0.8.1

- **TUI fixes** — two visual correctness fixes:
  - ROMs now show `waiting` in the target panel as soon as they are enqueued,
    not only when a worker actually picks them up. The Downloads panel now reflects
    the full queue, not just the 4 active slots.
  - The Prepare panel (PKGBUILD generation) was merged into Discovery — the phase
    is too fast to warrant its own column. ROMs in that sub-phase now appear in
    Discovery with status `preparing...`.

**Migration from v0.8.0:** none.

---

### v0.8.0

- **Reworked TUI** — replaced the flat spinner list with a proper panel-based interface
  built on `ratatui` + `crossterm`:
  - Top: scrolling **Completed** log (newest first) with a global progress gauge
  - Bottom: three side-by-side panels — **Discovery**, **Packaging**, **Downloads** —
    each showing only the ROMs currently active in that phase, with their own
    per-phase progress gauge
  - Accent colors per phase (cyan / yellow / green), rounded borders, bold labels,
    status text colored by state (active, done, error, queued)

**Migration from v0.7.0:** none — configuration file is unchanged.

---

### v0.7.0

- **Parallel pipeline** — discovery, packaging, and downloads now run concurrently.
  As soon as a ROM is discovered, it moves to packaging; as soon as it is packaged,
  it moves to download — without waiting for the rest of the queue.
  Discovery parallelism is automatically capped to the `maxthreads` limit of your
  ScreenScraper account.

**Migration from v0.6.0:** none — configuration file is unchanged.

---

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
