mod errors;
mod grid;
mod modal;
mod rate;
mod render;

use std::{
  io,
  sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
  },
  thread,
  time::{Duration, Instant},
};

use crossbeam_channel as channel;
use crossterm::{
  event::{Event, KeyCode, KeyEventKind, KeyModifiers},
  execute,
  terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::queue::TaskQueue;
use crate::summary::Summary;

use modal::show_modal;
use rate::Rate;
use render::render;

// ── Constants ─────────────────────────────────────────────────────────────

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const TICK_MS: u64 = 80;

/// How many assets a ROM is tracked against: the description plus eight media types.
pub(crate) const MEDIA_COUNT: usize = 9;

/// The canonical order of the nine tracked assets, and the Nerd Font glyph for each.
/// This order is the order of the media columns in the grid and of the coverage block
/// in the summary.
///
/// Without a Nerd Font installed these render as tofu — nine identical boxes — which is
/// the whole interface's worth of information gone. `--ascii` swaps the table.
const MEDIA_ICONS_NERD: &[(&str, &str)] = &[
  ("description", "󰗚"),
  ("video", "󰕧"),
  ("image", "󰋩"),
  ("thumbnail", "󰋫"),
  ("screenshot", "󰹙"),
  ("bezel", "󱂬"),
  ("marquee", "󰯃"),
  ("wheel", "󰊢"),
  ("manual", "󰂺"),
];

/// One ASCII letter per asset, same order. The letters are not all initials — marquee
/// and manual collide — so the coverage block of the summary is what makes them
/// readable, and it is generated from this very table.
const MEDIA_ICONS_ASCII: &[(&str, &str)] = &[
  ("description", "D"),
  ("video", "V"),
  ("image", "I"),
  ("thumbnail", "T"),
  ("screenshot", "S"),
  ("bezel", "B"),
  ("marquee", "Q"),
  ("wheel", "W"),
  ("manual", "M"),
];

/// Set once by `main` when `--ascii` is given, before any thread reads it.
static ASCII_ICONS: AtomicBool = AtomicBool::new(false);

/// Switches the process to the ASCII table.
///
/// Process-wide rather than threaded through `Ui`, `render` and `Summary`, because the
/// choice is made once on the command line and never changes: carrying it through three
/// layers would be three parameters that can only ever hold one value.
pub fn use_ascii_icons() {
  ASCII_ICONS.store(true, Ordering::Relaxed);
}

/// The icon table in force — the renderer and the summary both go through this.
pub(crate) fn media_icons() -> &'static [(&'static str, &'static str)] {
  if ASCII_ICONS.load(Ordering::Relaxed) {
    MEDIA_ICONS_ASCII
  } else {
    MEDIA_ICONS_NERD
  }
}

/// Position of an asset in the canonical order, which is the index of its grid column.
///
/// Both tables list the same nine kinds in the same order, so the index does not depend
/// on which one is in force.
fn media_index(kind: &str) -> Option<usize> {
  MEDIA_ICONS_NERD.iter().position(|&(k, _)| k == kind)
}

/// Set once by `main`, from `--plain` or from stdout not being a terminal.
static PLAIN: AtomicBool = AtomicBool::new(false);

/// Drops the full-screen interface for one line per finished ROM.
///
/// `Ui::new` unconditionally called `enable_raw_mode().unwrap()`. Where there is no
/// controlling terminal — a CI runner, a detached process — crossterm cannot open
/// `/dev/tty` and that unwrap panics on the render thread. `install_panic_hook()`
/// suppresses panic output so it would not be painted over the interface, so the failure
/// was completely silent: no interface, no error, and a run that went all the way through
/// reporting nothing at all.
pub fn use_plain_output() {
  PLAIN.store(true, Ordering::Relaxed);
}

pub(crate) fn is_plain() -> bool {
  PLAIN.load(Ordering::Relaxed)
}

// ── Modal public types ─────────────────────────────────────────────────────

/// One game candidate returned by `jeu_recherche`, for display in the modal.
///
/// Everything here comes out of the `JeuInfo` the search already returned — the API
/// documents `jeuRecherche` as "identical to jeuInfos but without the ROM information",
/// so the media list, the publisher and the genre cost no extra call. The design handoff
/// budgeted one `jeuInfos` per candidate for this.
#[derive(Clone)]
pub struct ModalCandidate {
  pub name: String,
  pub game_id: String,
  pub year: Option<String>,
  /// Which of the nine tracked assets this candidate has, in `MEDIA_ICONS` order.
  pub media: [Dot; MEDIA_COUNT],
  pub publisher: Option<String>,
  pub genre: Option<String>,
  pub players: Option<String>,
  pub region: Option<String>,
}

/// Request sent by a discovery worker when a ROM cannot be identified.
pub struct ModalRequest {
  /// Which grid row is waiting on this. Carried rather than matched on the file name:
  /// the row is what the "to identify" view selects, and a name can be rewritten by an
  /// identification landing in between.
  pub row: usize,
  pub filename: String,
  pub sha1: Option<String>,
  pub candidates: Vec<ModalCandidate>,
  pub response: channel::Sender<ModalResponse>,
  /// Called when the user types a game ID manually and presses Enter.
  ///
  /// `Err` carries the reason to show inline. It matters that it is not an `Option`:
  /// "ScreenScraper has no game 12345" and "ScreenScraper is unreachable" ask the user
  /// for opposite things — retype the ID, or stop typing and check the network.
  pub fetch_by_id: Box<dyn Fn(u32) -> Result<String, String> + Send>,
}

/// User response from the modal.
pub enum ModalResponse {
  /// User selected one of the search candidates (returns its SS game ID).
  SelectedId(String),
  /// User typed a game ID manually (raw string, may need parsing).
  ManualId(String),
  /// User skipped this ROM.
  Cancelled,
}

// ── Grid alphabet ──────────────────────────────────────────────────────────

/// State of one pipeline stage in a grid row: identification, packaging, ROM transfer.
///
/// This replaces the old `RomPhase`, which said *where* a ROM was so it could be routed
/// to one panel or the other. The grid asks a different question — a ROM sits on one
/// line for the whole run, and each column says how far that one stage got.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Cell {
  /// Not reached yet.
  Todo,
  /// Running now.
  Running,
  /// Running, but blocked on the user rather than on the machine.
  Waiting,
  Done,
  /// Nothing to do: identical to the last run.
  Unchanged,
  Failed,
}

/// State of one media asset in a grid row.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Dot {
  /// Not attempted yet.
  Todo,
  Running,
  /// Fetched during this run.
  Fresh,
  /// Already on disk with the right sha1.
  Unchanged,
  /// ScreenScraper has none.
  Missing,
}

impl Dot {
  /// Whether the package ends up with this asset, however it got there.
  fn present(self) -> bool {
    matches!(self, Dot::Fresh | Dot::Unchanged)
  }
}

// ── Modal internal state ───────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum ModalMode {
  List,
  Input,
  /// The user typed a game ID and we fetched its name; waiting for confirmation.
  Confirming {
    game_id: String,
    game_name: String,
  },
}

/// Display state stored in `AppState` so the render function can draw the modal.
pub(crate) struct ModalDisplayState {
  /// The grid row being identified, so the modal title names the same number the
  /// "to identify" view does.
  row: usize,
  filename: String,
  sha1: Option<String>,
  candidates: Vec<ModalCandidate>,
  cursor: usize,
  input: String,
  mode: ModalMode,
  /// Error message shown below the input field (e.g. "ID not found").
  input_status: Option<String>,
}

// ── App state ──────────────────────────────────────────────────────────────

/// Which rows the grid is showing.
///
/// `Errors` draws its own block with its own columns; the other two are the same grid
/// over a shorter list.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Filter {
  All,
  /// Started and not finished — what the workers are on right now.
  Active,
  Errors,
  /// Blocked on the user, and holding a blocking-pool worker while they wait.
  Unidentified,
}

impl Filter {
  fn next(self) -> Filter {
    match self {
      Filter::All => Filter::Active,
      Filter::Active => Filter::Errors,
      Filter::Errors => Filter::Unidentified,
      Filter::Unidentified => Filter::All,
    }
  }

  /// Whether this filter draws its own block instead of the grid and the banner.
  pub(crate) fn is_focused(self) -> bool {
    matches!(self, Filter::Errors | Filter::Unidentified)
  }
}

/// What the grid knows about a ROM before the pipeline has touched it.
///
/// Passed whole to `new_rom_bar` rather than as five positional arguments, three of
/// which are strings that would be trivial to swap at the call site.
pub struct RomInfo {
  /// Logical name: the file name, or the group name for a multi-disc game.
  pub label: String,
  /// The file this row actually comes from — the IA path or the local path.
  pub file_name: String,
  /// Known up front for an IA source; computed by `ComputeHashes` for a folder one.
  pub size: Option<u64>,
  pub sha1: Option<String>,
  /// Where it comes from, for the detail lines: `archive.org/<item>` or a directory.
  pub source: String,
}

/// One ROM, for the whole run. There is exactly one of these per ROM and it never
/// moves: its index in `AppState::roms` is its arrival order, which is its row.
pub(crate) struct RomEntry {
  /// Scraped name once identified, file name until then.
  pub(crate) label: String,
  pub(crate) file_name: String,
  pub(crate) size: Option<u64>,
  pub(crate) sha1: Option<String>,
  pub(crate) source: String,
  /// How many candidates `jeu_recherche` returned, once it has run.
  pub(crate) candidates: Option<usize>,
  /// The candidate ScreenScraper ranked first. Its own ordering is the only ranking
  /// there is — `jeuRecherche` sorts by probability and returns no score.
  pub(crate) best_candidate: Option<String>,
  /// Since when this ROM has been blocked on the user.
  pub(crate) waiting_since: Option<Instant>,
  pub(crate) status: String,
  pub(crate) id: Cell,
  pub(crate) pkg: Cell,
  pub(crate) rom: Cell,
  pub(crate) media: [Dot; MEDIA_COUNT],
  /// When the first step touched this ROM. `None` while it is still queued, which is
  /// what the `time` column shows as `—`.
  pub(crate) started_at: Option<Instant>,
  pub(crate) finished_at: Option<Instant>,
  /// Why it failed. `None` on success, and on a failure restored from a `run.yml`
  /// written before the cause was recorded.
  pub(crate) error: Option<String>,
  /// Nothing changed since the last run: ROM, media and description.xml all identical.
  pub(crate) unchanged: bool,
  /// How many times a step has been attempted. One until a retry says otherwise.
  pub(crate) attempts: u8,
}

impl RomEntry {
  /// A ROM as it enters the grid: on screen, in arrival order, nothing done yet.
  pub(crate) fn queued(info: RomInfo) -> Self {
    RomEntry {
      label: info.label,
      file_name: info.file_name,
      size: info.size,
      sha1: info.sha1,
      source: info.source,
      candidates: None,
      best_candidate: None,
      waiting_since: None,
      status: "queued".to_string(),
      id: Cell::Todo,
      pkg: Cell::Todo,
      rom: Cell::Todo,
      media: [Dot::Todo; MEDIA_COUNT],
      started_at: None,
      finished_at: None,
      error: None,
      unchanged: false,
      attempts: 1,
    }
  }

  pub(crate) fn finished(&self) -> bool {
    self.finished_at.is_some()
  }

  pub(crate) fn failed(&self) -> bool {
    self.error.is_some()
  }

  /// Wall-clock time spent on this ROM so far, or in total once it is finished.
  pub(crate) fn elapsed(&self) -> Option<Duration> {
    let start = self.started_at?;
    Some(match self.finished_at {
      Some(end) => end.saturating_duration_since(start),
      None => start.elapsed(),
    })
  }
}

pub(crate) struct AppState {
  pub(crate) roms: Vec<RomEntry>,
  pub(crate) total: usize,
  /// The system being scraped, for the banner title.
  pub(crate) system: String,
  /// Shown in place of the grid while the sources are still being collected.
  pub(crate) header: String,
  pub(crate) tick: usize,
  /// Row the cursor is on. Its details are unfolded underneath it.
  pub(crate) selected: usize,
  /// First visible row. Owned by the renderer, which is the only thing that knows how
  /// tall the grid is.
  pub(crate) scroll: usize,
  /// Whether the window tracks the workers on its own. Moving the cursor turns this
  /// off — the user is reading something — and `G` turns it back on.
  pub(crate) follow: bool,
  pub(crate) filter: Filter,
  /// One-off message shown in place of the key hints — what `w` just wrote, or why it
  /// could not. Cleared by the next keypress, so it never becomes stale furniture.
  pub(crate) notice: Option<String>,
  /// When the run started, which is what the throughput window is keyed on.
  pub(crate) started: Instant,
  /// Bytes written by ROM and media transfers so far.
  ///
  /// Counted per finished file, not per chunk: neither `internetarchive` nor
  /// `screenscraper` reports a download's progress (see the handoffs in
  /// `.claude/plans/v0.19.0/`), so the figure advances in steps rather than smoothly.
  pub(crate) bytes: u64,
  pub(crate) rate: Rate,
  /// Workers currently inside a step handler, and how many there are in total.
  pub(crate) workers: Option<(Arc<AtomicUsize>, usize)>,
  /// When set, the render function draws the modal overlay.
  pub(crate) modal: Option<ModalDisplayState>,
  /// Identification requests waiting for the user, oldest first.
  ///
  /// A worker that sends one blocks on its response channel until it is answered, so a
  /// parked request is a blocked blocking-pool worker. That is the cost of letting more
  /// than one ROM wait at a time, and why `N_BLOCKING_WORKERS` is what it is.
  pub(crate) pending: Vec<ModalRequest>,
  /// A row whose parked request the user asked to open. Set by the key handler and
  /// consumed by the render loop, which is the only place that owns the terminal.
  pub(crate) open_row: Option<usize>,
}

impl AppState {
  pub(crate) fn done(&self) -> usize {
    self.roms.iter().filter(|r| r.finished()).count()
  }
}

// ── Public types ───────────────────────────────────────────────────────────

/// Handle to one ROM's entry in the shared state.
/// Each method corresponds to a pipeline transition; the render thread
/// reads the resulting state autonomously.
pub struct RomBar {
  state: Arc<Mutex<AppState>>,
  index: usize,
}

impl RomBar {
  /// Makes the shared UI state usable again after a step handler panicked while
  /// holding its lock. Without this the render thread would panic on its next frame
  /// and the interface would freeze mid-run.
  pub fn clear_poison(&self) {
    self.state.clear_poison();
  }
}

pub struct Ui {
  state: Arc<Mutex<AppState>>,
  running: Arc<AtomicBool>,
  render_handle: Option<thread::JoinHandle<()>>,
  modal_tx: channel::Sender<ModalRequest>,
}

// ── RomBar ─────────────────────────────────────────────────────────────────

impl RomBar {
  /// Sets the status phrase, and starts the clock if this is the first thing that ever
  /// happened to this ROM.
  fn set_status(&self, status: impl Into<String>) {
    let mut s = self.state.lock().unwrap();
    let entry = &mut s.roms[self.index];
    entry.status = status.into();
    if entry.started_at.is_none() {
      entry.started_at = Some(Instant::now());
    }
  }

  fn set_media(&self, kind: &str, dot: Dot) {
    if let Some(i) = media_index(kind) {
      self.state.lock().unwrap().roms[self.index].media[i] = dot;
    }
  }

  /// What `ComputeHashes` found. An IA row already has both from the item metadata;
  /// a folder row only learns them here.
  pub fn set_hashes(&self, sha1: Option<String>, size: u64) {
    let mut s = self.state.lock().unwrap();
    let entry = &mut s.roms[self.index];
    entry.sha1 = sha1;
    if size > 0 {
      entry.size = Some(size);
    }
  }

  /// What the name search came back with: how many, and the one ScreenScraper put first.
  pub fn set_candidates(&self, n: usize, best: Option<String>) {
    let mut s = self.state.lock().unwrap();
    let entry = &mut s.roms[self.index];
    entry.candidates = Some(n);
    entry.best_candidate = best;
  }

  /// Which grid row this bar drives, so a `ModalRequest` can name it.
  pub fn row(&self) -> usize {
    self.index
  }

  // ── Identification ──────────────────────────────────────────────────────

  pub fn discovering(&self) {
    self.state.lock().unwrap().roms[self.index].id = Cell::Running;
    self.set_status("identifying");
  }

  /// The step failed on something transient and will be tried again after a backoff.
  ///
  /// Without this the row kept whatever status it had while the worker slept 1, then 2,
  /// then 4 seconds. From the outside the ROM was simply frozen, and a run slowed down
  /// by a flaky network looked identical to one blocked on something else entirely.
  pub fn retrying(&self, attempt: u8, max: u8) {
    self.state.lock().unwrap().roms[self.index].attempts = attempt.saturating_add(1);
    self.set_status(format!("retrying ({}/{})...", attempt, max));
  }

  pub fn found(&mut self, name: &str) {
    let mut s = self.state.lock().unwrap();
    let entry = &mut s.roms[self.index];
    entry.label = name.to_string();
    entry.id = Cell::Done;
    entry.status = "identified".to_string();
  }

  /// The user skipped identification: the package is built without metadata.
  ///
  /// Marked failed rather than done — the ROM completes, but its `description.xml` is
  /// empty and that is worth a red cell on the row for the rest of the run.
  pub fn not_found(&self) {
    self.state.lock().unwrap().roms[self.index].id = Cell::Failed;
    self.set_status("not identified");
  }

  /// The worker is blocked on the user, not on the machine.
  pub fn waiting_for_user(&self) {
    {
      let mut s = self.state.lock().unwrap();
      let entry = &mut s.roms[self.index];
      entry.id = Cell::Waiting;
      entry.waiting_since = Some(Instant::now());
    }
    self.set_status("to identify");
  }

  // ── Packaging ───────────────────────────────────────────────────────────

  pub fn queued_for_packaging(&self) {
    self.set_status("queued");
  }

  pub fn preparing(&self) {
    self.state.lock().unwrap().roms[self.index].pkg = Cell::Running;
    self.set_status("packaging");
  }

  /// The PKGBUILD step is over: written, or left alone because nothing changed.
  pub fn pkg_done(&self, unchanged: bool) {
    self.state.lock().unwrap().roms[self.index].pkg = if unchanged {
      Cell::Unchanged
    } else {
      Cell::Done
    };
  }

  // ── ROM transfer ────────────────────────────────────────────────────────

  pub fn queued_for_download(&self) {
    self.set_status("queued");
  }

  pub fn rom_checking(&self) {
    self.state.lock().unwrap().roms[self.index].rom = Cell::Running;
    self.set_status("checking ROM");
  }

  pub fn rom_downloading(&self) {
    self.state.lock().unwrap().roms[self.index].rom = Cell::Running;
    self.set_status("downloading ROM");
  }

  pub fn rom_redownloading(&self) {
    self.state.lock().unwrap().roms[self.index].rom = Cell::Running;
    self.set_status("checksum mismatch, re-downloading");
  }

  /// `bytes` is the size of the file that was just written — the only measure of
  /// transfer volume available while the libraries report nothing during a download.
  pub fn rom_done(&self, bytes: u64) {
    let mut s = self.state.lock().unwrap();
    s.roms[self.index].rom = Cell::Done;
    s.bytes += bytes;
  }

  pub fn rom_skipped(&self) {
    self.state.lock().unwrap().roms[self.index].rom = Cell::Unchanged;
  }

  // ── Media ───────────────────────────────────────────────────────────────

  pub fn start_media(&self, kind: &str) {
    self.set_media(kind, Dot::Running);
    self.set_status(format!("{} — downloading", kind));
  }

  pub fn media_done(&self, kind: &str, bytes: u64) {
    self.set_media(kind, Dot::Fresh);
    self.set_status(format!("{} ✓", kind));
    self.state.lock().unwrap().bytes += bytes;
  }

  pub fn media_skipped(&self, kind: &str) {
    self.set_media(kind, Dot::Unchanged);
    self.set_status(format!("{} — unchanged", kind));
  }

  pub fn media_unavailable(&self, kind: &str) {
    self.set_media(kind, Dot::Missing);
    self.set_status(format!("{} — not available", kind));
  }

  // ── End ─────────────────────────────────────────────────────────────────

  /// This ROM finished during the run that was interrupted, and its pipeline was
  /// restored from `run.yml` rather than executed.
  ///
  /// Without this the row would keep three `·` cells under a green name: a finished ROM
  /// that looks like it never started.
  pub fn restored(&self) {
    let mut s = self.state.lock().unwrap();
    let entry = &mut s.roms[self.index];
    entry.id = Cell::Done;
    entry.pkg = Cell::Done;
    entry.rom = Cell::Done;
    entry.started_at = Some(Instant::now());
  }

  pub fn finish(&self, unchanged: bool) {
    self.complete(None, unchanged);
  }

  /// `cause` is what `StepStatus::Failed` carried. It is the only trace of the failure
  /// that survives the run: the step is gone from memory by the time the summary prints.
  pub fn finish_error(&self, cause: &str) {
    self.complete(Some(cause), false);
  }

  /// Closes the row, and in plain mode says so on stdout.
  ///
  /// The line is built under the lock and printed after it: `println!` takes the stdout
  /// lock, and holding both while nine workers are finishing is a queue nobody needs.
  fn complete(&self, cause: Option<&str>, unchanged: bool) {
    let line = {
      let mut s = self.state.lock().unwrap();
      let done = s.done() + 1;
      let total = s.total;
      let entry = &mut s.roms[self.index];

      entry.finished_at = Some(Instant::now());
      entry.unchanged = unchanged;
      match cause {
        Some(cause) => {
          entry.error = Some(cause.to_string());
          entry.status = cause.replace('\n', " ");
          // The first stage that never completed is the one that broke. Testing for
          // "still running" instead would leave a row restored from `run.yml` with no
          // red cell at all: nothing is running on a resumed ROM.
          if let Some(cell) = [&mut entry.id, &mut entry.pkg, &mut entry.rom]
            .into_iter()
            .find(|c| !matches!(**c, Cell::Done | Cell::Unchanged))
          {
            *cell = Cell::Failed;
          }
        }
        None => {
          entry.status = if unchanged { "unchanged" } else { "done" }.to_string();
        }
      }

      if is_plain() {
        Some(plain_line(entry, done, total))
      } else {
        None
      }
    };

    if let Some(line) = line {
      println!("{}", line);
    }
  }
}

/// One line per finished ROM, for `--plain`.
///
/// Carries the same three things a grid row does — the marker, the name, and then
/// either which assets the package has or why it failed — plus the running count,
/// which the interface gets from its gauge.
fn plain_line(entry: &RomEntry, done: usize, total: usize) -> String {
  let marker = if entry.failed() {
    "✗"
  } else if entry.unchanged {
    "="
  } else {
    "✓"
  };

  let tail = if entry.failed() {
    entry
      .error
      .as_deref()
      .map(|cause| format!("  {}", cause.replace('\n', " ")))
      .unwrap_or_default()
  } else {
    let present: Vec<&str> = media_icons()
      .iter()
      .enumerate()
      .filter(|(i, _)| entry.media[*i].present())
      .map(|(_, &(_, icon))| icon)
      .collect();
    if present.is_empty() {
      String::new()
    } else {
      format!("  {}", present.join(" "))
    }
  };

  format!("[{}/{}] {} {}{}", done, total, marker, entry.label, tail)
}

/// Writes `<system>.errors.log` next to the other run files, and says what happened.
///
/// Into the working directory, like `state.yml` and `run.yml`: that is where rompom
/// already writes, and where the user is standing when they press the key.
fn write_errors_log(state: &AppState) -> String {
  let failures: Vec<(String, String)> = state
    .roms
    .iter()
    .filter(|r| r.failed())
    .map(|r| {
      (
        r.label.clone(),
        r.error
          .clone()
          .unwrap_or_else(|| "unknown cause".to_string()),
      )
    })
    .collect();

  let path = format!("{}.errors.log", state.system);
  match std::fs::write(&path, errors::log(&state.system, &failures)) {
    Ok(()) => format!("wrote {} ({} failures)", path, failures.len()),
    // Said on screen rather than swallowed: a keypress that silently does nothing is
    // indistinguishable from one that is not bound.
    Err(e) => format!("could not write {}: {}", path, e),
  }
}

/// Row indices the current filter shows, in arrival order.
pub(crate) fn visible_rows(state: &AppState) -> Vec<usize> {
  state
    .roms
    .iter()
    .enumerate()
    .filter(|(_, r)| match state.filter {
      Filter::All => true,
      Filter::Active => r.started_at.is_some() && !r.finished(),
      Filter::Errors => r.failed(),
      Filter::Unidentified => r.id == Cell::Waiting,
    })
    .map(|(i, _)| i)
    .collect()
}

/// Moves the cursor and switches filters.
///
/// `selected` is an index into `roms`, not into what is on screen: a ROM keeps its
/// identity across a filter change, so leaving the errors view puts the cursor back on
/// the same ROM rather than on whatever now sits in that position.
///
/// Any deliberate cursor move turns following off — the user is reading a particular
/// row, and a list that keeps sliding under the cursor cannot be read. `G` is the way
/// back: it means "take me to where the run is", which is what following does.
fn navigate(state: &Mutex<AppState>, code: KeyCode) {
  let mut s = state.lock().unwrap();
  s.notice = None;

  match code {
    KeyCode::Char('f') => s.filter = s.filter.next(),
    KeyCode::Char('e') => s.filter = Filter::Errors,
    KeyCode::Char('m') => s.filter = Filter::Unidentified,
    KeyCode::Char('w') if s.filter == Filter::Errors => {
      s.notice = Some(write_errors_log(&s));
      return;
    }
    // Handing the row to the render loop rather than opening here: `show_modal` owns
    // the terminal, and this runs on the same thread but outside the draw.
    KeyCode::Enter if s.filter == Filter::Unidentified => {
      s.open_row = Some(s.selected);
      return;
    }
    KeyCode::Char('s') if s.filter == Filter::Unidentified => {
      let row = s.selected;
      if let Some(i) = s.pending.iter().position(|r| r.row == row) {
        let req = s.pending.remove(i);
        let _ = req.response.send(ModalResponse::Cancelled);
      }
      return;
    }
    KeyCode::Esc => s.filter = Filter::All,
    _ => {}
  }

  let rows = visible_rows(&s);
  if rows.is_empty() {
    return;
  }
  // A filter change can hide the selected ROM; land on the nearest visible one rather
  // than on a cursor pointing at a row that is not drawn.
  let pos = rows
    .iter()
    .position(|&i| i == s.selected)
    .unwrap_or_else(|| {
      rows
        .partition_point(|&i| i < s.selected)
        .min(rows.len() - 1)
    });

  let pos = match code {
    KeyCode::Up => {
      s.follow = false;
      pos.saturating_sub(1)
    }
    KeyCode::Down => {
      s.follow = false;
      (pos + 1).min(rows.len() - 1)
    }
    KeyCode::Char('g') => {
      s.follow = false;
      0
    }
    KeyCode::Char('G') => {
      s.follow = true;
      rows.len() - 1
    }
    _ => pos,
  };
  s.selected = rows[pos];
}

// ── Ui ─────────────────────────────────────────────────────────────────────

impl Ui {
  pub fn new(interrupted: Arc<AtomicBool>, queue: Arc<TaskQueue>) -> Self {
    let state = Arc::new(Mutex::new(AppState {
      roms: Vec::new(),
      total: 0,
      system: String::new(),
      header: String::from("Collecting..."),
      tick: 0,
      selected: 0,
      scroll: 0,
      follow: true,
      filter: Filter::All,
      notice: None,
      started: Instant::now(),
      bytes: 0,
      rate: Rate::new(),
      workers: None,
      modal: None,
      pending: Vec::new(),
      open_row: None,
    }));

    let running = Arc::new(AtomicBool::new(true));
    let state_r = Arc::clone(&state);
    let running_r = Arc::clone(&running);
    let interrupted_r = Arc::clone(&interrupted);
    let queue_r = Arc::clone(&queue);

    let (modal_tx, modal_rx) = channel::unbounded::<ModalRequest>();

    // No terminal to draw on, and nothing to poll for keys: Ctrl-C goes back to being an
    // ordinary SIGINT, which the `ctrlc` handler in `main` already covers. The modal
    // receiver is dropped with this branch — `handle_wait_modal` never sends in plain
    // mode, it fails the ROM instead.
    if is_plain() {
      drop(modal_rx);
      return Ui {
        state,
        running,
        render_handle: None,
        modal_tx,
      };
    }

    let render_handle = thread::spawn(move || {
      enable_raw_mode().unwrap();
      let mut stdout = io::stdout();
      execute!(stdout, EnterAlternateScreen).unwrap();
      let backend = CrosstermBackend::new(stdout);
      let mut terminal = Terminal::new(backend).unwrap();

      while running_r.load(Ordering::Relaxed) {
        state_r.lock().unwrap().tick += 1;
        terminal
          .draw(|frame| {
            let mut state = state_r.lock().unwrap();
            render(frame, &mut state);
          })
          .unwrap();

        // Park whatever arrived. The first one to turn up with nothing else waiting
        // opens on its own, so a run with a single unidentified ROM behaves as it
        // always has; once a queue has formed it is the user who decides when to work
        // through it, from the `m` view, while the rest of the run carries on.
        let mut open_now = false;
        while let Ok(req) = modal_rx.try_recv() {
          let mut s = state_r.lock().unwrap();
          open_now |= s.pending.is_empty() && s.modal.is_none();
          s.pending.push(req);
        }

        // A parked request is a worker blocked on a channel nobody will answer once the
        // interface is going away. Cancelling them is what lets the run join.
        if interrupted_r.load(Ordering::SeqCst) {
          for req in state_r.lock().unwrap().pending.drain(..) {
            let _ = req.response.send(ModalResponse::Cancelled);
          }
        }

        // Either the user picked a row in the "to identify" view, or a lone request
        // just turned up with nobody ahead of it.
        let req = {
          let mut s = state_r.lock().unwrap();
          match s.open_row.take() {
            Some(row) => s
              .pending
              .iter()
              .position(|r| r.row == row)
              .map(|i| s.pending.remove(i)),
            None if open_now => s.pending.pop(),
            None => None,
          }
        };

        if let Some(req) = req {
          show_modal(req, &mut terminal, &state_r, &interrupted_r, &queue_r);
        } else if crossterm::event::poll(Duration::from_millis(TICK_MS)).unwrap_or(false) {
          if let Ok(Event::Key(key)) = crossterm::event::read() {
            if key.kind != KeyEventKind::Press {
              continue;
            }
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
              if interrupted_r.swap(true, Ordering::SeqCst) {
                std::process::exit(1);
              }
              queue_r.shutdown();
            } else {
              navigate(&state_r, key.code);
            }
          }
        }
      }

      disable_raw_mode().unwrap();
      execute!(terminal.backend_mut(), LeaveAlternateScreen).unwrap();
    });

    Ui {
      state,
      running,
      render_handle: Some(render_handle),
      modal_tx,
    }
  }

  /// Hands the banner the live worker count, so it can show how much of the pool is
  /// actually busy rather than how big the pool is.
  pub fn set_workers(&self, active: Arc<AtomicUsize>, total: usize) {
    self.state.lock().unwrap().workers = Some((active, total));
  }

  /// Names the run in the banner title.
  pub fn set_system(&self, name: &str) {
    self.state.lock().unwrap().system = name.to_string();
  }

  pub fn fetching_metadata(&self, item: &str) {
    self.state.lock().unwrap().header = format!("Fetching metadata: {}", item);
  }

  /// Appends a row. Its index is `roms.len()`, which is the arrival order the grid is
  /// ordered by and never re-sorted from.
  /// `total` is recorded so the banner can show `done/total`.
  pub fn new_rom_bar(&self, total: usize, info: RomInfo) -> RomBar {
    let mut s = self.state.lock().unwrap();
    s.total = total;
    let bar_index = s.roms.len();
    s.roms.push(RomEntry::queued(info));
    RomBar {
      state: Arc::clone(&self.state),
      index: bar_index,
    }
  }

  /// Returns a sender that discovery workers can use to request user
  /// identification of an unrecognised ROM.
  pub fn modal_sender(&self) -> channel::Sender<ModalRequest> {
    self.modal_tx.clone()
  }

  /// Extract end-of-run statistics. Call before dropping `Ui`, print after.
  pub fn summary(&self) -> Summary {
    let s = self.state.lock().unwrap();
    let finished = || s.roms.iter().filter(|r| r.finished());
    let success = finished().filter(|r| !r.failed()).count();
    let unchanged = finished().filter(|r| !r.failed() && r.unchanged).count();
    let errors = finished().filter(|r| r.failed()).count();
    let media_stats = media_icons()
      .iter()
      .enumerate()
      .map(|(i, &(kind, icon))| {
        let found = finished()
          .filter(|r| !r.failed() && r.media[i].present())
          .count();
        (kind, icon, found)
      })
      .collect();
    // In arrival order: the grid is read top to bottom, and so is a printed list.
    let failures = finished()
      .filter(|r| r.failed())
      .map(|r| {
        (
          r.label.clone(),
          r.error
            .clone()
            .unwrap_or_else(|| "unknown cause".to_string()),
        )
      })
      .collect();
    Summary {
      total: s.total,
      success,
      unchanged,
      errors,
      failures,
      media_stats,
      step_avg_durations: Vec::new(),
    }
  }
}

impl Drop for Ui {
  fn drop(&mut self) {
    self.running.store(false, Ordering::Relaxed);
    if let Some(h) = self.render_handle.take() {
      h.join().ok();
    }
  }
}
