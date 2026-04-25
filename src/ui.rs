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

use crossbeam_channel as channel;
use crossterm::{
  event::{Event, KeyCode, KeyEventKind},
  execute,
  terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
  backend::CrosstermBackend,
  layout::{Constraint, Direction, Layout, Rect},
  style::{Color, Modifier, Style},
  text::{Line, Span},
  widgets::{Block, BorderType, Borders, Clear, Gauge, List, ListItem, Paragraph},
  Frame, Terminal,
};

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
  /// Should return the game's canonical name if found, or `None` if not.
  /// The call is blocking; the TUI is redrawn with a "looking up…" indicator
  /// before the call so the user sees feedback.
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
/// Does not hold the response channel — that lives in `show_modal`.
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
          .filter(|e| e.media_found.iter().any(|k| k == kind))
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

// ── Modal logic ────────────────────────────────────────────────────────────

/// Blocking modal interaction loop. Runs inside the render thread.
///
/// - Drains any buffered key events before opening the modal.
/// - Redraws at ~50 ms intervals while waiting for user input.
/// - Sends exactly one `ModalResponse` before returning.
fn show_modal(
  req: ModalRequest,
  terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
  state: &Arc<Mutex<AppState>>,
) {
  // Drain buffered events so stray keypresses don't close the modal immediately.
  while crossterm::event::poll(Duration::ZERO).unwrap_or(false) {
    let _ = crossterm::event::read();
  }

  let mut cursor: usize = 0;
  let mut input = String::new();
  let mut mode = ModalMode::List;
  let mut input_status: Option<String> = None;

  loop {
    // Update modal display state before drawing.
    {
      let mut s = state.lock().unwrap();
      s.tick += 1;
      s.modal = Some(ModalDisplayState {
        filename: req.filename.clone(),
        sha1: req.sha1.clone(),
        candidates: req.candidates.clone(),
        cursor,
        input: input.clone(),
        mode: mode.clone(),
        input_status: input_status.clone(),
      });
    }

    terminal
      .draw(|frame| {
        let s = state.lock().unwrap();
        render(frame, &s);
      })
      .unwrap();

    if crossterm::event::poll(Duration::from_millis(50)).unwrap_or(false) {
      if let Ok(Event::Key(key)) = crossterm::event::read() {
        if key.kind != KeyEventKind::Press {
          continue;
        }
        match mode.clone() {
          ModalMode::List => match key.code {
            KeyCode::Up => cursor = cursor.saturating_sub(1),
            KeyCode::Down => {
              if cursor + 1 < req.candidates.len() {
                cursor += 1;
              }
            }
            KeyCode::Enter => {
              let resp = if req.candidates.is_empty() {
                ModalResponse::Cancelled
              } else {
                ModalResponse::SelectedId(req.candidates[cursor].game_id.clone())
              };
              let _ = req.response.send(resp);
              state.lock().unwrap().modal = None;
              return;
            }
            KeyCode::Char('i') => {
              mode = ModalMode::Input;
              input_status = None;
            }
            KeyCode::Esc => {
              let _ = req.response.send(ModalResponse::Cancelled);
              state.lock().unwrap().modal = None;
              return;
            }
            _ => {}
          },

          ModalMode::Input => match key.code {
            KeyCode::Enter => {
              if let Ok(game_id) = input.parse::<u32>() {
                // Show "looking up…" before the blocking API call.
                {
                  let mut s = state.lock().unwrap();
                  if let Some(ref mut m) = s.modal {
                    m.input_status = Some("Looking up…".to_string());
                  }
                }
                terminal
                  .draw(|frame| {
                    let s = state.lock().unwrap();
                    render(frame, &s);
                  })
                  .unwrap();

                match (req.fetch_by_id)(game_id) {
                  Some(name) => {
                    mode = ModalMode::Confirming {
                      game_id: input.clone(),
                      game_name: name,
                    };
                    input_status = None;
                  }
                  None => {
                    input_status = Some("ID not found on ScreenScraper".to_string());
                  }
                }
              }
            }
            KeyCode::Esc => {
              mode = ModalMode::List;
              input.clear();
              input_status = None;
            }
            KeyCode::Backspace => {
              input.pop();
              input_status = None;
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
              input.push(c);
              input_status = None;
            }
            _ => {}
          },

          ModalMode::Confirming { game_id, .. } => match key.code {
            KeyCode::Enter => {
              let _ = req.response.send(ModalResponse::ManualId(game_id));
              state.lock().unwrap().modal = None;
              return;
            }
            KeyCode::Esc => {
              // Go back to input mode keeping the typed ID.
              mode = ModalMode::Input;
              input_status = None;
            }
            _ => {}
          },
        }
      }
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

  if let Some(ref modal) = state.modal {
    render_modal(frame, frame.area(), modal);
  }
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
    .constraints([
      Constraint::Length(1),
      Constraint::Min(0),
      Constraint::Length(1),
    ])
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

  let legend_spans: Vec<Span> = MEDIA_ICONS
    .iter()
    .flat_map(|&(kind, icon)| {
      [
        Span::styled(format!("{} ", icon), Style::default().fg(Color::DarkGray)),
        Span::styled(
          format!("{}  ", kind),
          Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
        ),
      ]
    })
    .collect();
  frame.render_widget(Line::from(legend_spans), chunks[2]);
}

fn completed_item(entry: &CompletedEntry) -> ListItem<'static> {
  let (check, label_color, label_modifier) = if !entry.success {
    ("✗  ", Color::Red, Modifier::BOLD)
  } else if entry.unchanged {
    ("=  ", Color::DarkGray, Modifier::empty())
  } else {
    ("✓  ", Color::Green, Modifier::BOLD)
  };

  let mut spans = vec![
    Span::styled(check, Style::default().fg(label_color)),
    Span::styled(
      entry.label.clone(),
      Style::default()
        .fg(label_color)
        .add_modifier(label_modifier),
    ),
    Span::raw("  "),
  ];

  // Media icons in canonical order:
  //   green   = downloaded (new or updated)
  //   gray    = already up-to-date (unchanged)
  //   red     = not available on ScreenScraper
  for &(kind, icon) in MEDIA_ICONS {
    let style = if entry.media_found.iter().any(|k| k == kind) {
      Style::default().fg(Color::Green)
    } else if entry.media_unchanged.iter().any(|k| k == kind) {
      Style::default().fg(Color::DarkGray)
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
  } else if status.contains("waiting for identification") {
    Style::default().fg(Color::Yellow)
  } else {
    Style::default().fg(accent)
  }
}

// ── Modal rendering ────────────────────────────────────────────────────────

fn render_modal(frame: &mut Frame, area: Rect, modal: &ModalDisplayState) {
  let popup = centered_rect(78, 72, area);
  frame.render_widget(Clear, popup);

  let block = Block::default()
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .border_style(Style::default().fg(Color::Yellow))
    .title(" ROM not identified — manual selection ")
    .title_style(
      Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD),
    );

  let inner = block.inner(popup);
  frame.render_widget(block, popup);

  // Layout: [file info] [sep] [candidates] [sep] [controls / input]
  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Length(2), // filename + sha1
      Constraint::Length(1), // empty separator
      Constraint::Min(2),    // candidate list
      Constraint::Length(1), // empty separator
      Constraint::Length(2), // keyboard hints / input line
    ])
    .split(inner);

  // — File info ——————————————————————————————————————————————————————————————
  let info = vec![
    Line::from(vec![
      Span::styled("File : ", Style::default().fg(Color::DarkGray)),
      Span::styled(
        modal.filename.clone(),
        Style::default().add_modifier(Modifier::BOLD),
      ),
    ]),
    Line::from(vec![
      Span::styled("SHA1 : ", Style::default().fg(Color::DarkGray)),
      Span::styled(
        modal.sha1.clone().unwrap_or_else(|| "—".to_string()),
        Style::default().fg(Color::DarkGray),
      ),
    ]),
  ];
  frame.render_widget(Paragraph::new(info), chunks[0]);

  // — Candidate list ─────────────────────────────────────────────────────────
  if modal.candidates.is_empty() {
    frame.render_widget(
      Paragraph::new(Line::from(Span::styled(
        "No results from ScreenScraper. Press i to enter a game ID manually, or Esc to skip.",
        Style::default().fg(Color::DarkGray),
      ))),
      chunks[2],
    );
  } else {
    let items: Vec<ListItem> = modal
      .candidates
      .iter()
      .enumerate()
      .map(|(i, c)| {
        let selected = i == modal.cursor;
        let arrow = if selected { "▶  " } else { "   " };
        let name_style = if selected {
          Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
        } else {
          Style::default()
        };
        let meta_style = Style::default().fg(Color::DarkGray);
        let year = c.year.as_deref().unwrap_or("????");
        ListItem::new(Line::from(vec![
          Span::styled(arrow.to_string(), name_style),
          Span::styled(format!("{:<50}", &c.name), name_style),
          Span::styled(format!("  [id:{:>6}]  {}", c.game_id, year), meta_style),
        ]))
      })
      .collect();
    frame.render_widget(List::new(items), chunks[2]);
  }

  // — Controls / input / confirmation ──────────────────────────────────────
  match &modal.mode {
    ModalMode::List => {
      let hints = vec![
        Line::from(vec![
          Span::styled("↑↓", Style::default().fg(Color::Yellow)),
          Span::raw(" navigate  "),
          Span::styled("Enter", Style::default().fg(Color::Yellow)),
          Span::raw(" confirm  "),
          Span::styled("i", Style::default().fg(Color::Yellow)),
          Span::raw(" type game ID  "),
          Span::styled("Esc", Style::default().fg(Color::Yellow)),
          Span::raw(" skip"),
        ]),
        Line::default(),
      ];
      frame.render_widget(Paragraph::new(hints), chunks[4]);
    }

    ModalMode::Input => {
      let status_line = match &modal.input_status {
        Some(msg) => Line::from(Span::styled(msg.clone(), Style::default().fg(Color::Red))),
        None => Line::from(vec![
          Span::styled("Enter", Style::default().fg(Color::Yellow)),
          Span::raw(" look up  "),
          Span::styled("Esc", Style::default().fg(Color::Yellow)),
          Span::raw(" back to list"),
        ]),
      };
      let lines = vec![
        Line::from(vec![
          Span::styled("Game ID: ", Style::default().fg(Color::Yellow)),
          Span::styled(
            modal.input.clone(),
            Style::default().add_modifier(Modifier::BOLD),
          ),
          Span::styled("█", Style::default().fg(Color::Yellow)),
        ]),
        status_line,
      ];
      frame.render_widget(Paragraph::new(lines), chunks[4]);
    }

    ModalMode::Confirming { game_id, game_name } => {
      // In Confirming mode, replace the candidate list area with the found game info.
      let confirm_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Green))
        .title(" Game found ")
        .title_style(
          Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        );
      let confirm_inner = confirm_block.inner(chunks[2]);
      frame.render_widget(Clear, chunks[2]);
      frame.render_widget(confirm_block, chunks[2]);

      let found_lines = vec![
        Line::from(vec![
          Span::styled("Name : ", Style::default().fg(Color::DarkGray)),
          Span::styled(
            game_name.clone(),
            Style::default()
              .fg(Color::Green)
              .add_modifier(Modifier::BOLD),
          ),
        ]),
        Line::from(vec![
          Span::styled("ID   : ", Style::default().fg(Color::DarkGray)),
          Span::styled(game_id.clone(), Style::default().fg(Color::Green)),
        ]),
      ];
      frame.render_widget(Paragraph::new(found_lines), confirm_inner);

      let hints = vec![
        Line::from(vec![
          Span::styled("Enter", Style::default().fg(Color::Green)),
          Span::raw(" confirm  "),
          Span::styled("Esc", Style::default().fg(Color::Yellow)),
          Span::raw(" back to input"),
        ]),
        Line::default(),
      ];
      frame.render_widget(Paragraph::new(hints), chunks[4]);
    }
  }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
  let vert = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Percentage((100 - percent_y) / 2),
      Constraint::Percentage(percent_y),
      Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

  Layout::default()
    .direction(Direction::Horizontal)
    .constraints([
      Constraint::Percentage((100 - percent_x) / 2),
      Constraint::Percentage(percent_x),
      Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vert[1])[1]
}
