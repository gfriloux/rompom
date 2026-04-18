use std::{
  io,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
  },
  thread,
  time::Duration,
};

use crossterm::{
  execute,
  terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
  backend::CrosstermBackend,
  layout::{Constraint, Direction, Layout, Rect},
  style::{Color, Style},
  widgets::{Block, Borders, List, ListItem},
  Frame, Terminal,
};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const TICK_MS: u64 = 80;
/// Height (in terminal lines) reserved for the active-phase panels at the bottom.
const PANEL_HEIGHT: u16 = 12;

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

/// Associates a display title with the phase it represents.
/// The renderer iterates `PANELS` dynamically — no match arms to update.
struct PanelDef {
  matches: fn(&RomPhase) -> bool,
  title: &'static str,
}

/// Ordered active-phase panels.
/// Add an entry here to get a new column in the TUI automatically.
const PANELS: &[PanelDef] = &[
  PanelDef {
    matches: |p| matches!(p, RomPhase::Discovering),
    title: "Discovery",
  },
  PanelDef {
    matches: |p| matches!(p, RomPhase::Packaging),
    title: "Packaging",
  },
  PanelDef {
    matches: |p| matches!(p, RomPhase::Downloading),
    title: "Downloads",
  },
];

// ── State ──────────────────────────────────────────────────────────────────

struct RomEntry {
  label: String,
  status: String,
  phase: RomPhase,
}

struct AppState {
  roms: Vec<RomEntry>,
  total: usize,
  /// Finished ROM lines, newest first.
  completed: Vec<String>,
  /// Shown in the completed panel when no ROM has finished yet.
  header: String,
  tick: usize,
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

  // Phase 2 — Packaging
  pub fn packaging(&self) {
    self.transition(RomPhase::Packaging, "packaging...");
  }

  // Phase 3 — ROM download
  pub fn rom_checking(&self) {
    self.transition(RomPhase::Downloading, "checking...");
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
    self.set_status(format!("{} ✓", kind));
  }

  pub fn media_unavailable(&self, kind: &str) {
    self.set_status(format!("{} — not available", kind));
  }

  // End
  pub fn finish(&self) {
    let mut s = self.state.lock().unwrap();
    let label = s.roms[self.index].label.clone();
    s.roms[self.index].phase = RomPhase::Done { success: true };
    s.completed.insert(0, format!("✓  {}", label));
  }

  pub fn finish_error(&self) {
    let mut s = self.state.lock().unwrap();
    let label = s.roms[self.index].label.clone();
    s.roms[self.index].phase = RomPhase::Done { success: false };
    s.completed.insert(0, format!("✗  {} — error", label));
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
    }));

    let running = Arc::new(AtomicBool::new(true));
    let state_r = Arc::clone(&state);
    let running_r = Arc::clone(&running);

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
        thread::sleep(Duration::from_millis(TICK_MS));
      }

      disable_raw_mode().unwrap();
      execute!(terminal.backend_mut(), LeaveAlternateScreen).unwrap();
    });

    Ui {
      state,
      running,
      render_handle: Some(render_handle),
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
    });
    RomBar {
      state: Arc::clone(&self.state),
      index: bar_index,
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

// ── Rendering ──────────────────────────────────────────────────────────────

fn render(frame: &mut Frame, state: &AppState) {
  let areas = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Min(1), Constraint::Length(PANEL_HEIGHT)])
    .split(frame.area());

  render_completed(frame, areas[0], state);
  render_active(frame, areas[1], state);
}

fn render_completed(frame: &mut Frame, area: Rect, state: &AppState) {
  let done = state.completed.len();
  let title = if state.total > 0 {
    format!(" Completed ({}/{}) ", done, state.total)
  } else {
    " Completed ".to_string()
  };

  let items: Vec<ListItem> = if state.completed.is_empty() {
    vec![ListItem::new(state.header.as_str())]
  } else {
    state
      .completed
      .iter()
      .map(|line| {
        let style = if line.starts_with('✓') {
          Style::default().fg(Color::Green)
        } else {
          Style::default().fg(Color::Red)
        };
        ListItem::new(line.as_str()).style(style)
      })
      .collect()
  };

  frame.render_widget(
    List::new(items).block(Block::default().borders(Borders::ALL).title(title)),
    area,
  );
}

fn render_active(frame: &mut Frame, area: Rect, state: &AppState) {
  if PANELS.is_empty() {
    return;
  }

  let n = PANELS.len() as u32;
  let constraints: Vec<Constraint> = (0..PANELS.len()).map(|_| Constraint::Ratio(1, n)).collect();

  let panel_areas = Layout::default()
    .direction(Direction::Horizontal)
    .constraints(constraints)
    .split(area);

  let spinner = SPINNER_FRAMES[state.tick % SPINNER_FRAMES.len()];

  for (i, panel) in PANELS.iter().enumerate() {
    let entries: Vec<&RomEntry> = state
      .roms
      .iter()
      .filter(|r| (panel.matches)(&r.phase))
      .collect();
    render_panel(frame, panel_areas[i], panel.title, &entries, spinner);
  }
}

fn render_panel(frame: &mut Frame, area: Rect, title: &str, entries: &[&RomEntry], spinner: &str) {
  let title_str = format!(" {} ({}) ", title, entries.len());

  let items: Vec<ListItem> = entries
    .iter()
    .map(|e| ListItem::new(format!("{} {} — {}", spinner, e.label, e.status)))
    .collect();

  frame.render_widget(
    List::new(items).block(Block::default().borders(Borders::ALL).title(title_str)),
    area,
  );
}
