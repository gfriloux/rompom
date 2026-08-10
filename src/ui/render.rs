use ratatui::{
  layout::{Constraint, Direction, Layout, Rect},
  style::{Color, Modifier, Style},
  text::{Line, Span},
  widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
  Frame,
};

use super::grid::{self, Columns};
use super::{
  media_icons, AppState, Cell, Dot, ModalDisplayState, ModalMode, RomEntry, MEDIA_COUNT,
  SPINNER_FRAMES,
};

/// Cells taken by the progress bar in the banner.
const BAR_WIDTH: usize = 40;

// ── Top-level render ────────────────────────────────────────────────────────

pub(super) fn render(frame: &mut Frame, state: &AppState) {
  let areas = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Length(3), // banner
      Constraint::Min(1),    // grid
      Constraint::Length(1), // key hints
    ])
    .split(frame.area());

  render_banner(frame, areas[0], state);
  render_grid(frame, areas[1], state);
  frame.render_widget(help_line(), areas[2]);

  if let Some(ref modal) = state.modal {
    render_modal(frame, frame.area(), modal);
  }
}

// ── Shared widget helpers ─────────────────────────────────────────────────

fn styled_block(title: String, color: Color) -> Block<'static> {
  Block::default()
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .border_style(Style::default().fg(color))
    .title(title)
    .title_style(Style::default().fg(color).add_modifier(Modifier::BOLD))
}

fn dim() -> Style {
  Style::default().fg(Color::DarkGray)
}

// ── Banner ────────────────────────────────────────────────────────────────

fn render_banner(frame: &mut Frame, area: Rect, state: &AppState) {
  let title = if state.system.is_empty() {
    " rompom ".to_string()
  } else {
    format!(" rompom · {} · {} roms ", state.system, state.total)
  };
  let block = styled_block(title, Color::Cyan);
  let inner = block.inner(area);
  frame.render_widget(block, area);

  let done = state.done();
  let ratio = if state.total > 0 {
    done as f64 / state.total as f64
  } else {
    0.0
  };
  let filled = (ratio * BAR_WIDTH as f64).round() as usize;

  // Two spans rather than a `Gauge`: the bar is 40 cells wide whatever the terminal is,
  // because the counters after it sit at fixed offsets and a stretching bar would push
  // them around at every resize.
  let mut spans = vec![
    Span::styled(grid::fit("progress", 10), dim()),
    Span::styled("█".repeat(filled), Style::default().fg(Color::Green)),
    Span::styled("█".repeat(BAR_WIDTH - filled), dim()),
    Span::styled(format!(" {:>3}% ", (ratio * 100.0) as u64), dim()),
  ];
  spans.extend(counter_spans(state));

  frame.render_widget(Paragraph::new(Line::from(spans)), inner);
}

/// `✓ N new   = N same   ✗ N failed   ? N to id`, at fixed offsets.
fn counter_spans(state: &AppState) -> Vec<Span<'static>> {
  let finished = || state.roms.iter().filter(|r| r.finished());
  let new = finished().filter(|r| !r.failed() && !r.unchanged).count();
  let same = finished().filter(|r| !r.failed() && r.unchanged).count();
  let failed = finished().filter(|r| r.failed()).count();
  let to_id = state.roms.iter().filter(|r| r.id == Cell::Waiting).count();

  vec![
    Span::styled(
      grid::fit(&format!("✓ {} new", new), 15),
      Style::default().fg(Color::Green),
    ),
    Span::raw(grid::fit(&format!("= {} same", same), 15)),
    Span::styled(
      grid::fit(&format!("✗ {} failed", failed), 14),
      Style::default().fg(Color::Red),
    ),
    Span::styled(
      format!("? {} to id", to_id),
      Style::default().fg(Color::Yellow),
    ),
  ]
}

// ── Grid ──────────────────────────────────────────────────────────────────

fn render_grid(frame: &mut Frame, area: Rect, state: &AppState) {
  let title = if state.total > 0 {
    format!(" roms · arrival order · {}/{} ", state.done(), state.total)
  } else {
    " roms ".to_string()
  };
  let block = styled_block(title, Color::White);
  let inner = block.inner(area);
  frame.render_widget(block, area);

  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Length(1), // column headers
      Constraint::Length(1), // rule
      Constraint::Min(0),    // rows
      Constraint::Length(1), // rule
      Constraint::Length(1), // footer
    ])
    .split(inner);

  let cols = grid::columns(inner.width);
  let rule = Span::styled("─".repeat(inner.width as usize), dim());

  frame.render_widget(Paragraph::new(header_line(&cols)), chunks[0]);
  frame.render_widget(Paragraph::new(Line::from(rule.clone())), chunks[1]);

  if state.roms.is_empty() {
    frame.render_widget(
      Paragraph::new(Line::from(Span::styled(
        state.header.clone(),
        dim().add_modifier(Modifier::ITALIC),
      ))),
      chunks[2],
    );
  } else {
    let height = chunks[2].height as usize;
    let anchor = grid::active_anchor(&state.roms);
    let offset = grid::scroll_offset(state.roms.len(), height, anchor);
    let spinner = SPINNER_FRAMES[state.tick % SPINNER_FRAMES.len()];

    let items: Vec<ListItem> = state
      .roms
      .iter()
      .enumerate()
      .skip(offset)
      .take(height)
      .map(|(i, entry)| ListItem::new(row_line(i, entry, &cols, spinner)))
      .collect();
    frame.render_widget(List::new(items), chunks[2]);

    frame.render_widget(Paragraph::new(Line::from(rule)), chunks[3]);
    frame.render_widget(
      Paragraph::new(footer_line(state.roms.len(), height, offset)),
      chunks[4],
    );
  }
}

fn header_line(cols: &Columns) -> Line<'static> {
  let mut spans = vec![
    Span::styled(grid::fit("#", cols.index as usize), dim()),
    Span::styled(grid::fit("rom", cols.name as usize), dim()),
    Span::styled(grid::fit("id", cols.id as usize), dim()),
    Span::styled(grid::fit("pkg", cols.pkg as usize), dim()),
    Span::styled(grid::fit("rom", cols.rom as usize), dim()),
  ];
  // The media columns are named by their own icon — nine headers of one cell each.
  for &(_, icon) in media_icons() {
    spans.push(Span::styled(
      grid::fit(icon, cols.media_cell as usize),
      dim(),
    ));
  }
  spans.push(Span::styled(grid::fit("time", cols.time as usize), dim()));
  spans.push(Span::styled("status".to_string(), dim()));
  Line::from(spans)
}

fn row_line(index: usize, entry: &RomEntry, cols: &Columns, spinner: &str) -> Line<'static> {
  let mut spans = vec![
    Span::styled(
      grid::fit(&(index + 1).to_string(), cols.index as usize),
      dim(),
    ),
    Span::styled(
      grid::fit(&entry.label, cols.name as usize),
      name_style(entry),
    ),
    cell_span(entry.id, cols.id as usize, spinner),
    cell_span(entry.pkg, cols.pkg as usize, spinner),
    cell_span(entry.rom, cols.rom as usize, spinner),
  ];

  for i in 0..MEDIA_COUNT {
    spans.push(dot_span(entry.media[i], cols.media_cell as usize));
  }

  let time = entry
    .elapsed()
    .map(grid::format_elapsed)
    .unwrap_or_else(|| "—".to_string());
  spans.push(Span::styled(grid::fit(&time, cols.time as usize), dim()));
  spans.push(Span::styled(
    grid::truncate(&entry.status, cols.status as usize),
    status_style(entry),
  ));

  Line::from(spans)
}

fn cell_span(cell: Cell, width: usize, spinner: &str) -> Span<'static> {
  let (glyph, color) = match cell {
    Cell::Todo => ("·", Color::DarkGray),
    Cell::Running => (spinner, Color::Cyan),
    Cell::Waiting => (spinner, Color::Yellow),
    Cell::Done => ("✓", Color::Green),
    Cell::Unchanged => ("=", Color::DarkGray),
    Cell::Failed => ("✗", Color::Red),
  };
  Span::styled(grid::fit(glyph, width), Style::default().fg(color))
}

fn dot_span(dot: Dot, width: usize) -> Span<'static> {
  let (glyph, color) = match dot {
    Dot::Todo => ("·", Color::DarkGray),
    Dot::Running => ("◐", Color::Green),
    Dot::Fresh => ("●", Color::Green),
    Dot::Unchanged => ("●", Color::DarkGray),
    Dot::Missing => ("○", Color::Red),
  };
  Span::styled(grid::fit(glyph, width), Style::default().fg(color))
}

/// The name carries the outcome, so a row can be read without looking at its cells.
fn name_style(entry: &RomEntry) -> Style {
  if entry.failed() {
    Style::default().fg(Color::Red)
  } else if entry.finished() && entry.unchanged {
    dim()
  } else if entry.finished() {
    Style::default().fg(Color::Green)
  } else if entry.started_at.is_some() {
    Style::default().add_modifier(Modifier::BOLD)
  } else {
    dim()
  }
}

fn status_style(entry: &RomEntry) -> Style {
  if entry.failed() {
    Style::default().fg(Color::Red)
  } else if entry.id == Cell::Waiting || entry.status.starts_with("retrying") {
    Style::default().fg(Color::Yellow)
  } else if entry.finished() || entry.started_at.is_none() {
    dim()
  } else {
    Style::default().fg(Color::Cyan)
  }
}

/// What is off screen, then what the dots mean.
///
/// The media *kinds* are named by the header icons, not here — the nine dots of a row
/// only ever need the five states explaining.
fn footer_line(len: usize, height: usize, offset: usize) -> Line<'static> {
  let below = len.saturating_sub(offset + height);
  let scroll = format!("↑ {} above · {} queued ↓", offset, below);

  let mut spans = vec![Span::styled(grid::fit(&scroll, 34), dim())];
  for (glyph, label, color) in [
    ("●", " fetched  ", Color::Green),
    ("●", " up to date  ", Color::DarkGray),
    ("○", " missing  ", Color::Red),
    ("◐", " in progress  ", Color::Green),
    ("·", " not tried", Color::DarkGray),
  ] {
    spans.push(Span::styled(glyph, Style::default().fg(color)));
    spans.push(Span::styled(label, dim()));
  }
  Line::from(spans)
}

fn help_line() -> Paragraph<'static> {
  Paragraph::new(Line::from(vec![
    Span::styled("ctrl-c", Style::default().fg(Color::Cyan)),
    Span::styled(" stop", dim()),
  ]))
}

// ── Modal rendering ────────────────────────────────────────────────────────

pub(super) fn render_modal(frame: &mut Frame, area: Rect, modal: &ModalDisplayState) {
  use ratatui::widgets::Clear;

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
      Span::styled("File : ", dim()),
      Span::styled(
        modal.filename.clone(),
        Style::default().add_modifier(Modifier::BOLD),
      ),
    ]),
    Line::from(vec![
      Span::styled("SHA1 : ", dim()),
      Span::styled(modal.sha1.clone().unwrap_or_else(|| "—".to_string()), dim()),
    ]),
  ];
  frame.render_widget(Paragraph::new(info), chunks[0]);

  // — Candidate list ─────────────────────────────────────────────────────────
  if modal.candidates.is_empty() {
    frame.render_widget(
      Paragraph::new(Line::from(Span::styled(
        "No results from ScreenScraper. Press i to enter a game ID manually, or Esc to skip.",
        dim(),
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
        let year = c.year.as_deref().unwrap_or("????");
        ListItem::new(Line::from(vec![
          Span::styled(arrow.to_string(), name_style),
          Span::styled(format!("{:<50}", &c.name), name_style),
          Span::styled(format!("  [id:{:>6}]  {}", c.game_id, year), dim()),
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
      use ratatui::widgets::Clear;

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
          Span::styled("Name : ", dim()),
          Span::styled(
            game_name.clone(),
            Style::default()
              .fg(Color::Green)
              .add_modifier(Modifier::BOLD),
          ),
        ]),
        Line::from(vec![
          Span::styled("ID   : ", dim()),
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
