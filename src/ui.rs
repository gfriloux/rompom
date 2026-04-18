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
  style::{Color, Modifier, Style},
  widgets::{Block, BorderType, Borders, Gauge, List, ListItem},
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
/// Add an entry here to get a new column in the TUI automatically.
const PANELS: &[PanelDef] = &[
  PanelDef {
    matches: |p| matches!(p, RomPhase::Discovering),
    past: |p| !matches!(p, RomPhase::Discovering),
    title: "Discovery",
    color: Color::Cyan,
  },
  PanelDef {
    matches: |p| matches!(p, RomPhase::Packaging),
    past: |p| matches!(p, RomPhase::Downloading | RomPhase::Done { .. }),
    title: "Packaging",
    color: Color::Yellow,
  },
  PanelDef {
    matches: |p| matches!(p, RomPhase::Downloading),
    past: |p| matches!(p, RomPhase::Done { .. }),
    title: "Downloads",
    color: Color::Green,
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

fn styled_block(title: String, color: Color) -> Block<'static> {
  Block::default()
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .border_style(Style::default().fg(color))
    .title(title)
    .title_style(Style::default().fg(color).add_modifier(Modifier::BOLD))
}

fn render_completed(frame: &mut Frame, area: Rect, state: &AppState) {
  let done = state.completed.len();
  let title = if state.total > 0 {
    format!(" Completed ({}/{}) ", done, state.total)
  } else {
    " Completed ".to_string()
  };

  let block = styled_block(title, Color::White);
  let inner = block.inner(area);
  frame.render_widget(block, area);

  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(1), Constraint::Min(0)])
    .split(inner);

  let ratio = if state.total > 0 {
    done as f64 / state.total as f64
  } else {
    0.0
  };
  let gauge_label = format!("{}/{}", done, state.total);
  let gauge = Gauge::default()
    .gauge_style(Style::default().fg(Color::White).bg(Color::DarkGray))
    .ratio(ratio)
    .label(gauge_label);
  frame.render_widget(gauge, chunks[0]);

  let items: Vec<ListItem> = if state.completed.is_empty() {
    vec![ListItem::new(state.header.as_str()).style(
      Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::ITALIC),
    )]
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

  frame.render_widget(List::new(items), chunks[1]);
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
    let past_count = state.roms.iter().filter(|r| (panel.past)(&r.phase)).count();
    render_panel(
      frame,
      panel_areas[i],
      panel,
      &entries,
      spinner,
      past_count,
      state.total,
    );
  }
}

fn render_panel(
  frame: &mut Frame,
  area: Rect,
  panel: &PanelDef,
  entries: &[&RomEntry],
  spinner: &str,
  past_count: usize,
  total: usize,
) {
  let title = format!(" {} ({}) ", panel.title, entries.len());
  let block = styled_block(title, panel.color);
  let inner = block.inner(area);
  frame.render_widget(block, area);

  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(1), Constraint::Min(0)])
    .split(inner);

  let ratio = if total > 0 {
    past_count as f64 / total as f64
  } else {
    0.0
  };
  let gauge_label = format!("{}/{}", past_count, total);
  let gauge = Gauge::default()
    .gauge_style(Style::default().fg(panel.color).bg(Color::DarkGray))
    .ratio(ratio)
    .label(gauge_label);
  frame.render_widget(gauge, chunks[0]);

  let items: Vec<ListItem> = entries
    .iter()
    .map(|e| {
      let spinner_span =
        ratatui::text::Span::styled(spinner.to_string(), Style::default().fg(panel.color));
      let label_span = ratatui::text::Span::styled(
        format!(" {} ", e.label),
        Style::default().add_modifier(Modifier::BOLD),
      );
      let status_span = ratatui::text::Span::styled(
        format!("— {}", e.status),
        status_style(&e.status, panel.color),
      );
      ListItem::new(ratatui::text::Line::from(vec![
        spinner_span,
        label_span,
        status_span,
      ]))
    })
    .collect();

  frame.render_widget(List::new(items), chunks[1]);
}

/// Color a status message based on its content.
fn status_style(status: &str, accent: Color) -> Style {
  if status.contains('✓') {
    Style::default().fg(Color::Green)
  } else if status.contains("error") || status.contains("mismatch") || status.contains("not found")
  {
    Style::default().fg(Color::Red)
  } else if status == "queued" {
    Style::default().fg(Color::DarkGray)
  } else {
    Style::default().fg(accent)
  }
}
