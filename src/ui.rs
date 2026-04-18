use std::{
  io,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
  },
  thread,
  time::Duration,
};

use crate::summary::Summary;

use crossterm::{
  execute,
  terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
  backend::CrosstermBackend,
  layout::{Constraint, Direction, Layout, Rect},
  style::{Color, Modifier, Style},
  text::{Line, Span},
  widgets::{Block, BorderType, Borders, Gauge, List, ListItem},
  Frame, Terminal,
};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const TICK_MS: u64 = 80;
/// Height (in terminal lines) reserved for the active-phase panels at the bottom.
const PANEL_HEIGHT: u16 = 12;

/// Canonical media types with their display icon.
/// Order here is the order icons appear in the Completed log and summary.
pub(crate) const MEDIA_ICONS: &[(&str, &str)] = &[
  ("video", "󰕧"),
  ("image", "󰋩"),
  ("thumbnail", "󰋫"),
  ("screenshot", "󰹙"),
  ("bezel", "󱂬"),
  ("marquee", "󰯃"),
  ("wheel", "󰊢"),
  ("manual", "󰂺"),
];

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

// ── State ──────────────────────────────────────────────────────────────────

struct RomEntry {
  label: String,
  status: String,
  phase: RomPhase,
  media_found: Vec<String>,
  media_missing: Vec<String>,
}

/// One entry in the Completed log.
pub(crate) struct CompletedEntry {
  pub(crate) label: String,
  pub(crate) success: bool,
  pub(crate) media_found: Vec<String>,
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

  pub fn media_unavailable(&self, kind: &str) {
    let mut s = self.state.lock().unwrap();
    s.roms[self.index].status = format!("{} — not available", kind);
    s.roms[self.index].media_missing.push(kind.to_string());
  }

  // End
  pub fn finish(&self) {
    let mut s = self.state.lock().unwrap();
    let entry = &s.roms[self.index];
    let completed = CompletedEntry {
      label: entry.label.clone(),
      success: true,
      media_found: entry.media_found.clone(),
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
      media_found: entry.media_found.clone(),
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
      media_found: Vec::new(),
      media_missing: Vec::new(),
    });
    RomBar {
      state: Arc::clone(&self.state),
      index: bar_index,
    }
  }

  /// Extract end-of-run statistics. Call before dropping `Ui`, print after.
  pub fn summary(&self) -> Summary {
    let s = self.state.lock().unwrap();
    let success = s.completed.iter().filter(|e| e.success).count();
    let errors = s.completed.iter().filter(|e| !e.success).count();
    let media_stats = MEDIA_ICONS
      .iter()
      .map(|&(kind, icon)| {
        let found = s
          .completed
          .iter()
          .filter(|e| e.media_found.iter().any(|k| k == kind))
          .count();
        (kind, icon, found)
      })
      .collect();
    Summary {
      total: s.total,
      success,
      errors,
      media_stats,
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
    state.completed.iter().map(completed_item).collect()
  };

  frame.render_widget(List::new(items), chunks[1]);
}

fn completed_item(entry: &CompletedEntry) -> ListItem<'static> {
  let (check, label_color) = if entry.success {
    ("✓  ", Color::Green)
  } else {
    ("✗  ", Color::Red)
  };

  let mut spans = vec![
    Span::styled(check, Style::default().fg(label_color)),
    Span::styled(
      entry.label.clone(),
      Style::default()
        .fg(label_color)
        .add_modifier(Modifier::BOLD),
    ),
    Span::raw("  "),
  ];

  // Media icons in canonical order: green if found, red if missing.
  for &(kind, icon) in MEDIA_ICONS {
    let style = if entry.media_found.iter().any(|k| k == kind) {
      Style::default().fg(Color::Green)
    } else if entry.media_missing.iter().any(|k| k == kind) {
      Style::default().fg(Color::Red)
    } else {
      continue;
    };
    spans.push(Span::styled(format!("{} ", icon), style));
  }

  ListItem::new(Line::from(spans))
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
      let spinner_span = Span::styled(spinner.to_string(), Style::default().fg(panel.color));
      let label_span = Span::styled(
        format!(" {} ", e.label),
        Style::default().add_modifier(Modifier::BOLD),
      );
      let status_span = Span::styled(
        format!("— {}", e.status),
        status_style(&e.status, panel.color),
      );
      ListItem::new(Line::from(vec![spinner_span, label_span, status_span]))
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
  } else if status == "queued" || status == "waiting" {
    Style::default().fg(Color::DarkGray)
  } else {
    Style::default().fg(accent)
  }
}
