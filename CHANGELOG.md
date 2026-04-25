# Changelog

### v0.14.2

**Bug fixes — Ctrl-C and interrupted-run resume.**

- **Fix: Ctrl-C had no effect during a run** — `crossterm::enable_raw_mode()` clears the
  POSIX `ISIG` flag, so pressing Ctrl-C no longer generates `SIGINT`; the `ctrlc` signal
  handler never fired. Ctrl-C is now detected directly as a keyboard event inside the render
  thread. It also works inside the identification modal.

- **Fix: `.run.yml` was never written after Ctrl-C** — workers blocked in
  `Semaphore::acquire()` (waiting for a free ScreenScraper slot) could not be unblocked
  after `queue.shutdown()`. The main thread then hung indefinitely on `join()`, and the
  run-state file was never written. `Semaphore` is now interruptible: `acquire()` returns
  `false` when cancelled, and `cancel()` wakes all waiting threads.

- **Fix: resume crashed with "attempt to subtract with overflow"** — `apply_run_state()`
  decremented a step's `wait_for` counter even when its own predecessor had not completed
  (e.g. `WaitModal` is `Skipped` by default, so its successor `BuildPackage` was decremented
  on resume even though `LookupSS` had not run yet). When `LookupSS` then ran and dispatched
  `WaitModal`, a second decrement underflowed to `usize::MAX`, causing a panic.
  `apply_run_state` now guards each decrement: successors are only decremented if the step's
  own `wait_for` has already reached 0 in the restored state.

- **Fix: Discovery restarted from scratch on resume** — a post-handler interrupted check in
  `execute_step` reset every step that completed just as the interrupt arrived — including
  `LookupSS` that had finished successfully — back to `Pending`. On resume, these steps were
  re-run from the beginning. Steps that complete normally are now always saved as `Done`.
  Only handlers that are cancelled mid-way (via a semaphore cancel) return
  `Err("interrupted")`, which is the sole trigger for resetting a step to `Pending`.

- **Fix: resume UI showed all ROMs as "queued" in Discovery** — bars were re-initialized
  fresh on every start. On resume, already-completed ROMs now appear immediately in the
  Completed panel, ROMs awaiting downloads appear in the Downloads panel, and ROMs awaiting
  packaging appear in the Discovery panel (Packaging sub-phase). Only ROMs still in Discovery
  stay as "queued".

**Migration from v0.14.1:** none — configuration file is unchanged.

---

### v0.14.1

**Internal refactor — no user-visible behavior change.**

The four largest source files have been split into focused sub-modules:

- `worker.rs` → `worker/{mod, run_state, helpers, handlers/{discovery, packaging, downloads, save_state}}`
- `ui.rs` → `ui/{mod, render, modal}`
- `conf.rs` → `conf/{mod, update}`
- `rom.rs` → `rom/{mod, step, source}`

No migration required.

---

### v0.14.0

- **DAG-based pipeline** — the three separate thread pools (discovery / packaging / downloads)
  have been replaced by a unified `TaskQueue` and two generic worker pools. Each ROM's pipeline
  is now modeled as a directed acyclic graph (DAG) of `Step`s; each step is enqueued as soon
  as its predecessors complete, so parallelism is maximised without artificial phase barriers.

- **Interrupted-run resume** — on Ctrl-C, the current state of every ROM's pipeline is
  persisted to `<system>.run.yml`. On the next invocation, rompom offers to resume from where
  it left off. Selecting "no" deletes the file and starts fresh.

- **description.xml change tracking** — changes to `description.xml` (new metadata from
  ScreenScraper, genre/description update, etc.) now count as a package change: the `pkgver`
  is bumped and the description icon (`󰗚`) appears in the Completed log for that ROM.

- **Media icon colors** — the Completed log now distinguishes three states per media type:
  - Green — downloaded or updated this run
  - Gray — already up-to-date (SHA1 verified, unchanged)
  - Red — not available on ScreenScraper

- **`--debug` flag** — writes `<system>.debug.log` with per-ROM pipeline decisions
  (`[ComputeHashes]`, `[LookupSS]`, `[BuildPackage]`), useful for understanding why a package
  was rebuilt or skipped.

- **Summary: total media coverage** — the end-of-run "Media coverage" table now counts all
  ROMs that have the media asset present (downloaded or already up-to-date), not only those
  updated during the current run. A second consecutive run on an unchanged system now shows
  accurate coverage instead of zeros.

- **Fix: ROM extension stripped from package name** — the file extension was incorrectly
  included in the normalized package name (`pkgname`). It is now stripped before normalization.

- **Nix: package + Home Manager module** — rompom is now installable via Nix. A Home Manager
  module is available under `modules/home-manager/rompom`.

**Migration from v0.13.x:** none — configuration file is unchanged.

---

### v0.13.0

- **Incremental update support** — re-running rompom on an already-processed system now
  skips unchanged ROMs and media instead of redoing all work from scratch.
  A state file `<system>.state.yml` is written next to the game folders after each run,
  recording for each ROM: ScreenScraper game ID, ROM SHA1, and per-media SHA1s.

  On subsequent runs:
  - **Faster discovery** — ROMs already identified use `jeuinfo_by_gameid` (direct lookup
    by cached ID) instead of the slower checksum-based `jeuinfo` call.
  - **ROM skip** — if the ROM's SHA1 is unchanged, the copy or download is skipped entirely.
  - **Media skip** — media files already on disk with a matching SHA1 are not re-downloaded.
  - **Package skip** — if neither the ROM nor any media changed, the `PKGBUILD` and
    `description.xml` are not rewritten.
  - **`pkgver` bump** — when a package *is* updated (new or changed media, ROM changed),
    the existing `pkgver` is read from the `PKGBUILD` and incremented by 1 automatically.
    First-time packages start at `pkgver=1`.

- **SHA1 fast-skip for folder sources** — for local folder sources, if a ROM file's
  modification time and size are unchanged since the last run, its SHA1 is reused from
  the state file without reading the file. This significantly reduces processing time
  when re-running on a folder of large ROM files.

- **UI: unchanged vs updated** — the Completed panel now distinguishes:
  - `=` (gray) — package is fully unchanged, nothing was rewritten or re-downloaded
  - `✓` (green) — package is new or was updated
  The end-of-run summary also reports `updated` and `unchanged` counts separately.

**Migration from v0.12.x:** none — configuration file is unchanged.
The state file is created automatically on the first run after upgrading.

---

### v0.12.0

- **Interactive identification modal** — when a ROM cannot be identified automatically during
  discovery, rompom opens an interactive TUI modal instead of silently skipping it:
  - Lists up to 30 `jeuRecherche` candidates (name + year) for the user to pick from
  - `↑`/`↓` to navigate, `Enter` to confirm, `Esc` to cancel (ROM is skipped)
  - `i` switches to manual entry: type a ScreenScraper game ID, press `Enter` to preview
    the game name, then confirm or go back
  - Multiple unidentified ROMs queue up; the modal processes them one at a time

**Dependency:** requires `screenscraper` ≥ v0.6.0 (`jeu_recherche`, `jeuinfo_by_gameid`).

---

### v0.11.0

- **Local folder source** — systems can now load ROMs from a local directory instead of
  Internet Archive. Use `source.folder` with a `path` and one or more glob `filter` patterns.
  SHA1/MD5/CRC32 checksums are computed per-ROM in the discovery workers (not during collection),
  so the full ROM list appears immediately in the TUI.
- **`filter` is now a list** — both `internet_archive` and `folder` sources accept multiple
  glob patterns (e.g. `["*.zip", "*.7z"]`).
- **Config format change: `ia_items` → `source`** — the per-system `ia_items` field has been
  replaced by a `source` block. Run `rompom --update-config` to migrate automatically.

**Migration from v0.10.x — BREAKING:**

The `ia_items` field is no longer valid. If your `rompom.yml` still uses it, rompom will
refuse to start and tell you to run:

```
rompom --update-config
```

This migrates each `ia_items` entry to the new `source.internet_archive` format in place,
including converting `filter: "*.zip"` (string) to `filter: ["*.zip"]` (list).

---

### v0.10.0

- **Description language preference** — a new required `lang` field in `rompom.yml` controls
  the language used for game descriptions and genres (e.g. `[fr, en]` tries French first,
  falls back to English).
- **Media region follows the ROM** — media assets (screenshot, image, bezel, etc.) are now
  selected based on the ROM's own region rather than a fixed `fr`-first preference.
  A US ROM will get US assets; only if none exist does it fall back to `wor`, then `ss`.
- **Media icon legend** — the bottom line of the Completed panel now shows a compact legend
  (`󰕧 video  󰋩 image  …`) so icons are self-explanatory without leaving the TUI.

**Internal refactors (no user-visible behavior change):**

- **Template-based PKGBUILD generation** — PKGBUILD files are now generated via MiniJinja
  templates (`assets/templates/pkgbuild/`). System-specific build/package sections live in
  dedicated `.jinja` files instead of inline Rust strings, making them easier to read and modify.
- **Template-based launchers** — the OpenBOR launcher script is generated from
  `assets/templates/launcher/openbor.jinja` instead of being built by hand in Rust.
- **description.xml via quick-xml** — the `Game` struct is now serialized using
  `quick_xml::se::Serializer` (serde), replacing the previous manual string construction.
- **Naming convention** — `find_system` and `normalize_name` aligned with Rust verb-noun convention.

**Migration from v0.9.x — BREAKING:**

The `lang` field is now required. If your `rompom.yml` does not have it, rompom will refuse
to start and tell you to run:

```
rompom --update-config
```

This command interactively asks for your language preference and updates `rompom.yml` in place.

---

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
