mod modal;
mod render;

use std::{
  io,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
  },
  thread,
  time::Duration,
};

use crossbeam_channel as channel;
use crossterm::{
  event::{Event, KeyCode, KeyEventKind, KeyModifiers},
  execute,
  terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, style::Color, Terminal};

use crate::queue::TaskQueue;
use crate::summary::Summary;

use modal::show_modal;
use render::render;

// ── Constants ─────────────────────────────────────────────────────────────

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const TICK_MS: u64 = 80;
/// Height (in terminal lines) reserved for the active-phase panels at the bottom.
const PANEL_HEIGHT: u16 = 12;

/// The canonical order of the nine tracked assets, and the Nerd Font glyph for each.
/// This order is the order icons appear in the Completed log and in the summary.
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
/// and manual collide — so the legend line is what makes them readable, and it is
/// generated from this very table.
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
#[derive(Clone)]
pub struct ModalCandidate {
  pub name: String,
  pub game_id: String,
  pub year: Option<String>,
}

/// Request sent by a discovery worker when a ROM cannot be identified.
pub struct ModalRequest {
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

// ── Phase ──────────────────────────────────────────────────────────────────

/// Pipeline phase for a ROM.
///
/// To add a new phase:
///   1. Add a variant here.
///   2. Add an entry in `PANELS`.
///   3. Add the corresponding method(s) on `RomBar`.
#[derive(Clone, PartialEq)]
enum RomPhase {
  Discovering,
  Packaging,
  Downloading,
  Done { success: bool },
}

// ── Panel descriptors ──────────────────────────────────────────────────────

/// Associates a display title, accent color, phase matcher, and completion predicate.
/// The renderer iterates `PANELS` dynamically — no match arms to update.
struct PanelDef {
  matches: fn(&RomPhase) -> bool,
  /// Returns true if a ROM has already passed through (or past) this phase.
  past: fn(&RomPhase) -> bool,
  title: &'static str,
  color: Color,
}

/// Ordered active-phase panels.
const PANELS: &[PanelDef] = &[
  PanelDef {
    matches: |p| matches!(p, RomPhase::Discovering | RomPhase::Packaging),
    past: |p| matches!(p, RomPhase::Downloading | RomPhase::Done { .. }),
    title: "Discovery",
    color: Color::Cyan,
  },
  PanelDef {
    matches: |p| matches!(p, RomPhase::Downloading),
    past: |p| matches!(p, RomPhase::Done { .. }),
    title: "Downloads",
    color: Color::Green,
  },
];

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
struct ModalDisplayState {
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

struct RomEntry {
  label: String,
  status: String,
  phase: RomPhase,
  media_found: Vec<String>,
  media_unchanged: Vec<String>,
  media_missing: Vec<String>,
}

/// One entry in the Completed log.
pub(crate) struct CompletedEntry {
  pub(crate) label: String,
  pub(crate) success: bool,
  pub(crate) unchanged: bool,
  /// Why this ROM failed. `None` on success, and on a failure restored from a
  /// `run.yml` written before the cause was recorded.
  pub(crate) error: Option<String>,
  pub(crate) media_found: Vec<String>,
  pub(crate) media_unchanged: Vec<String>,
  pub(crate) media_missing: Vec<String>,
}

struct AppState {
  roms: Vec<RomEntry>,
  total: usize,
  /// Finished ROM entries, newest first.
  completed: Vec<CompletedEntry>,
  /// Shown in the completed panel when no ROM has finished yet.
  header: String,
  tick: usize,
  /// When set, the render function draws the modal overlay.
  modal: Option<ModalDisplayState>,
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
  fn set_status(&self, status: impl Into<String>) {
    self.state.lock().unwrap().roms[self.index].status = status.into();
  }

  fn transition(&self, phase: RomPhase, status: impl Into<String>) {
    let mut s = self.state.lock().unwrap();
    s.roms[self.index].phase = phase;
    s.roms[self.index].status = status.into();
  }

  // Phase 1 — Discovery
  pub fn discovering(&self) {
    self.set_status("discovering...");
  }

  /// The step failed on something transient and will be tried again after a backoff.
  ///
  /// Without this the bar kept whatever status it had while the worker slept 1, then 2,
  /// then 4 seconds. From the outside the ROM was simply frozen, and a run slowed down
  /// by a flaky network looked identical to one blocked on something else entirely.
  pub fn retrying(&self, attempt: u8, max: u8) {
    self.set_status(format!("retrying ({}/{})...", attempt, max));
  }

  pub fn found(&mut self, name: &str) {
    let mut s = self.state.lock().unwrap();
    s.roms[self.index].label = name.to_string();
    s.roms[self.index].status = "found".to_string();
  }

  pub fn not_found(&self) {
    self.set_status("not found");
  }

  /// The worker is waiting for the user to identify the ROM in the modal.
  pub fn waiting_for_user(&self) {
    self.set_status("waiting for identification...");
  }

  // Phase 2 — Packaging
  pub fn preparing_pending(&self) {
    self.transition(RomPhase::Packaging, "waiting");
  }

  pub fn preparing(&self) {
    self.set_status("preparing...");
  }

  // Phase 3 — ROM download
  pub fn downloading_pending(&self) {
    self.transition(RomPhase::Downloading, "waiting");
  }

  pub fn rom_checking(&self) {
    self.set_status("checking...");
  }

  pub fn rom_downloading(&self) {
    self.transition(RomPhase::Downloading, "downloading ROM...");
  }

  pub fn rom_redownloading(&self) {
    self.transition(
      RomPhase::Downloading,
      "checksum mismatch, re-downloading...",
    );
  }

  pub fn rom_done(&self) {
    self.set_status("ROM ✓");
  }

  pub fn rom_skipped(&self) {
    self.set_status("ROM ✓ (already exists)");
  }

  // Phase 3 — Media downloads
  pub fn start_media(&self, kind: &str) {
    self.set_status(format!("{} — downloading...", kind));
  }

  pub fn media_done(&self, kind: &str) {
    let mut s = self.state.lock().unwrap();
    s.roms[self.index].status = format!("{} ✓", kind);
    s.roms[self.index].media_found.push(kind.to_string());
  }

  pub fn media_skipped(&self, kind: &str) {
    let mut s = self.state.lock().unwrap();
    s.roms[self.index].status = format!("{} — unchanged", kind);
    s.roms[self.index].media_unchanged.push(kind.to_string());
  }

  pub fn media_unavailable(&self, kind: &str) {
    let mut s = self.state.lock().unwrap();
    s.roms[self.index].status = format!("{} — not available", kind);
    s.roms[self.index].media_missing.push(kind.to_string());
  }

  // End
  pub fn finish(&self, unchanged: bool) {
    self.complete(CompletedFrom::Success { unchanged });
  }

  /// `cause` is what `StepStatus::Failed` carried. It is the only trace of the failure
  /// that survives the run: the step is gone from memory by the time the summary prints.
  pub fn finish_error(&self, cause: &str) {
    self.complete(CompletedFrom::Failure { cause });
  }

  /// Moves this ROM into the Completed log, and in plain mode says so on stdout.
  ///
  /// The line is built under the lock and printed after it: `println!` takes the stdout
  /// lock, and holding both while nine workers are finishing is a queue nobody needs.
  fn complete(&self, outcome: CompletedFrom<'_>) {
    let line = {
      let mut s = self.state.lock().unwrap();
      let entry = &s.roms[self.index];
      let (success, unchanged, error) = match outcome {
        CompletedFrom::Success { unchanged } => (true, unchanged, None),
        CompletedFrom::Failure { cause } => (false, false, Some(cause.to_string())),
      };
      let completed = CompletedEntry {
        label: entry.label.clone(),
        success,
        unchanged,
        error,
        media_found: entry.media_found.clone(),
        media_unchanged: entry.media_unchanged.clone(),
        media_missing: entry.media_missing.clone(),
      };
      s.roms[self.index].phase = RomPhase::Done { success };
      s.completed.insert(0, completed);

      if is_plain() {
        Some(plain_line(&s.completed[0], s.completed.len(), s.total))
      } else {
        None
      }
    };

    if let Some(line) = line {
      println!("{}", line);
    }
  }
}

/// Which of the two endings a ROM reached, so `complete` can build the entry once.
enum CompletedFrom<'a> {
  Success { unchanged: bool },
  Failure { cause: &'a str },
}

/// One line per finished ROM, for `--plain`.
///
/// Carries the same three things the Completed panel does — the marker, the name, and
/// then either which assets the package has or why it failed — plus the running count,
/// which the panel gets from its gauge.
fn plain_line(entry: &CompletedEntry, done: usize, total: usize) -> String {
  let marker = if !entry.success {
    "✗"
  } else if entry.unchanged {
    "="
  } else {
    "✓"
  };

  let tail = if !entry.success {
    entry
      .error
      .as_deref()
      .map(|cause| format!("  {}", cause.replace('\n', " ")))
      .unwrap_or_default()
  } else {
    let present: Vec<&str> = media_icons()
      .iter()
      .filter(|(kind, _)| {
        entry.media_found.iter().any(|k| k == kind)
          || entry.media_unchanged.iter().any(|k| k == kind)
      })
      .map(|&(_, icon)| icon)
      .collect();
    if present.is_empty() {
      String::new()
    } else {
      format!("  {}", present.join(" "))
    }
  };

  format!("[{}/{}] {} {}{}", done, total, marker, entry.label, tail)
}

// ── Ui ─────────────────────────────────────────────────────────────────────

impl Ui {
  pub fn new(interrupted: Arc<AtomicBool>, queue: Arc<TaskQueue>) -> Self {
    let state = Arc::new(Mutex::new(AppState {
      roms: Vec::new(),
      total: 0,
      completed: Vec::new(),
      header: String::from("Collecting..."),
      tick: 0,
      modal: None,
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
            let state = state_r.lock().unwrap();
            render(frame, &state);
          })
          .unwrap();

        if let Ok(req) = modal_rx.try_recv() {
          show_modal(req, &mut terminal, &state_r, &interrupted_r, &queue_r);
        } else if crossterm::event::poll(Duration::from_millis(TICK_MS)).unwrap_or(false) {
          if let Ok(Event::Key(key)) = crossterm::event::read() {
            if key.kind == KeyEventKind::Press
              && key.code == KeyCode::Char('c')
              && key.modifiers.contains(KeyModifiers::CONTROL)
            {
              if interrupted_r.swap(true, Ordering::SeqCst) {
                std::process::exit(1);
              }
              queue_r.shutdown();
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

  pub fn fetching_metadata(&self, item: &str) {
    self.state.lock().unwrap().header = format!("Fetching metadata: {}", item);
  }

  /// `_index` is ignored — the bar index is assigned from `roms.len()`.
  /// `total` is recorded so the completed panel can show `done/total`.
  pub fn new_rom_bar(&self, _index: usize, total: usize, filename: &str) -> RomBar {
    let mut s = self.state.lock().unwrap();
    s.total = total;
    let bar_index = s.roms.len();
    s.roms.push(RomEntry {
      label: filename.to_string(),
      status: "queued".to_string(),
      phase: RomPhase::Discovering,
      media_found: Vec::new(),
      media_unchanged: Vec::new(),
      media_missing: Vec::new(),
    });
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
    let success = s.completed.iter().filter(|e| e.success).count();
    let unchanged = s.completed.iter().filter(|e| e.unchanged).count();
    let errors = s.completed.iter().filter(|e| !e.success).count();
    let media_stats = media_icons()
      .iter()
      .map(|&(kind, icon)| {
        let found = s
          .completed
          .iter()
          .filter(|e| {
            e.media_found.iter().any(|k| k == kind) || e.media_unchanged.iter().any(|k| k == kind)
          })
          .count();
        (kind, icon, found)
      })
      .collect();
    // Oldest first: the panel shows newest first because it scrolls, but a printed
    // list reads in the order the run produced it.
    let failures = s
      .completed
      .iter()
      .rev()
      .filter(|e| !e.success)
      .map(|e| {
        (
          e.label.clone(),
          e.error
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
