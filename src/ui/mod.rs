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
  execute,
  terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, style::Color, Terminal};

use crate::summary::Summary;

use modal::show_modal;
use render::render;

// ── Constants ─────────────────────────────────────────────────────────────

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const TICK_MS: u64 = 80;
/// Height (in terminal lines) reserved for the active-phase panels at the bottom.
const PANEL_HEIGHT: u16 = 12;

/// Canonical media types with their display icon.
/// Order here is the order icons appear in the Completed log and summary.
pub(crate) const MEDIA_ICONS: &[(&str, &str)] = &[
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
  pub fetch_by_id: Box<dyn Fn(u32) -> Option<String> + Send>,
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
    let mut s = self.state.lock().unwrap();
    let entry = &s.roms[self.index];
    let completed = CompletedEntry {
      label: entry.label.clone(),
      success: true,
      unchanged,
      media_found: entry.media_found.clone(),
      media_unchanged: entry.media_unchanged.clone(),
      media_missing: entry.media_missing.clone(),
    };
    s.roms[self.index].phase = RomPhase::Done { success: true };
    s.completed.insert(0, completed);
  }

  pub fn finish_error(&self) {
    let mut s = self.state.lock().unwrap();
    let entry = &s.roms[self.index];
    let completed = CompletedEntry {
      label: entry.label.clone(),
      success: false,
      unchanged: false,
      media_found: entry.media_found.clone(),
      media_unchanged: entry.media_unchanged.clone(),
      media_missing: entry.media_missing.clone(),
    };
    s.roms[self.index].phase = RomPhase::Done { success: false };
    s.completed.insert(0, completed);
  }
}

// ── Ui ─────────────────────────────────────────────────────────────────────

impl Ui {
  pub fn new() -> Self {
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

    let (modal_tx, modal_rx) = channel::unbounded::<ModalRequest>();

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
          show_modal(req, &mut terminal, &state_r);
        } else {
          thread::sleep(Duration::from_millis(TICK_MS));
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
    let media_stats = MEDIA_ICONS
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
    Summary {
      total: s.total,
      success,
      unchanged,
      errors,
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
