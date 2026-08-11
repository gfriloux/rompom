use std::sync::atomic::Ordering;

use ratatui::{
  layout::{Constraint, Direction, Layout, Rect},
  style::{Color, Modifier, Style},
  text::{Line, Span},
  widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
  Frame,
};

use super::grid::{self, Columns};
use super::{
  errors, media_icons, visible_rows, AppState, Cell, Dot, Filter, ModalDisplayState, ModalMode,
  RomEntry, MEDIA_COUNT, SPINNER_FRAMES,
};

/// Cells taken by the progress bar in the banner.
const BAR_WIDTH: usize = 40;

/// Cells taken by the throughput sparkline, under the left of the progress bar.
const SPARK_WIDTH: usize = 30;

/// Background of the selected row and of the detail lines under it.
const SELECTED_BG: Color = Color::Rgb(0x16, 0x1c, 0x24);

/// Same, in the yellow of the "to identify" view.
const WAITING_BG: Color = Color::Rgb(0x1c, 0x1a, 0x14);

// ── Top-level render ────────────────────────────────────────────────────────

/// Takes the state mutably because the scroll offset lives in it and the renderer is
/// the only thing that knows how tall the grid is on this frame.
pub(super) fn render(frame: &mut Frame, state: &mut AppState) {
  // The throughput window advances from the frame loop: it needs a heartbeat, and this
  // is the one thing that already has one.
  let (elapsed, done, bytes) = (state.started.elapsed(), state.done(), state.bytes);
  state.rate.tick(elapsed, done, bytes);

  // A filtered view hides the banner: it is a different question — "what went wrong" —
  // and the run-wide progress has nothing to say about it.
  let constraints: &[Constraint] = if state.filter.is_focused() {
    &[Constraint::Min(1), Constraint::Length(1)]
  } else {
    &[
      Constraint::Length(4), // banner
      Constraint::Min(1),    // grid
      Constraint::Length(1), // key hints
    ]
  };
  let areas = Layout::default()
    .direction(Direction::Vertical)
    .constraints(constraints)
    .split(frame.area());

  if state.filter == Filter::Errors {
    render_errors(frame, areas[0], state);
    frame.render_widget(hints_or_notice(state, errors_help_line()), areas[1]);
  } else if state.filter == Filter::Unidentified {
    render_unidentified(frame, areas[0], state);
    frame.render_widget(hints_or_notice(state, unidentified_help_line()), areas[1]);
  } else {
    render_banner(frame, areas[0], state);
    render_grid(frame, areas[1], state);
    frame.render_widget(hints_or_notice(state, help_line(state)), areas[2]);
  }

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
  let mut progress = vec![
    Span::styled(grid::fit("progress", 10), dim()),
    Span::styled("█".repeat(filled), Style::default().fg(Color::Green)),
    Span::styled("█".repeat(BAR_WIDTH - filled), dim()),
    Span::styled(format!(" {:>3}% ", (ratio * 100.0) as u64), dim()),
  ];
  progress.extend(counter_spans(state));

  frame.render_widget(
    Paragraph::new(vec![Line::from(progress), throughput_line(state, done)]),
    inner,
  );
}

/// `rate  ▄▅▆▇█…   29 rom/min   12.1 MiB/s   eta 4m 20s   7/8 workers`
fn throughput_line(state: &AppState, done: usize) -> Line<'static> {
  let eta = match state.rate.eta(state.total.saturating_sub(done)) {
    Some(d) => format!("eta {}", grid::format_elapsed(d)),
    // Nothing has finished in the last minute, so there is no honest number to put
    // here — and a made-up one is what people plan the evening around.
    None => "eta —".to_string(),
  };
  let workers = match &state.workers {
    Some((active, total)) => format!("{}/{} workers", active.load(Ordering::Relaxed), total),
    None => String::new(),
  };

  Line::from(vec![
    Span::styled(grid::fit("rate", 10), dim()),
    Span::styled(
      state.rate.spark(SPARK_WIDTH),
      Style::default().fg(Color::Cyan),
    ),
    Span::raw(grid::fit("", BAR_WIDTH - SPARK_WIDTH + 1)),
    Span::styled(
      grid::fit(&format!("{:.0} rom/min", state.rate.roms_per_min()), 15),
      Style::default().add_modifier(Modifier::BOLD),
    ),
    Span::styled(
      grid::fit(
        &format!(
          "{}/s",
          grid::format_bytes(state.rate.bytes_per_sec() as u64)
        ),
        15,
      ),
      dim(),
    ),
    Span::styled(grid::fit(&eta, 14), dim()),
    Span::styled(workers, dim()),
  ])
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

fn render_grid(frame: &mut Frame, area: Rect, state: &mut AppState) {
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
    let rows = visible_rows(state);
    let entries: Vec<&RomEntry> = rows.iter().map(|&i| &state.roms[i]).collect();
    let len = entries.len();
    if len == 0 {
      frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
          "nothing in flight".to_string(),
          dim().add_modifier(Modifier::ITALIC),
        ))),
        chunks[2],
      );
      return;
    }
    let cursor = rows.iter().position(|&i| i == state.selected).unwrap_or(0);

    // Following tracks the workers; once the user has moved the cursor the window only
    // budges when the selection would otherwise leave it.
    state.scroll = if state.follow {
      grid::scroll_offset(len, height, grid::active_anchor(&entries))
    } else {
      grid::clamp_scroll(state.scroll, len, height, cursor)
    };
    let offset = state.scroll;
    let spinner = SPINNER_FRAMES[state.tick % SPINNER_FRAMES.len()];

    // Built row by row rather than mapped, because the selected row is followed by
    // three detail lines that take rows from the same window.
    let mut items: Vec<ListItem> = Vec::with_capacity(height);
    for (pos, &row) in rows.iter().enumerate().skip(offset) {
      if items.len() >= height {
        break;
      }
      let entry = &state.roms[row];
      let selected = pos == cursor;
      items.push(ListItem::new(row_line(
        row, entry, &cols, spinner, selected,
      )));
      if selected {
        for line in detail_lines(entry, inner.width as usize) {
          if items.len() >= height {
            break;
          }
          items.push(ListItem::new(line));
        }
      }
    }
    frame.render_widget(List::new(items), chunks[2]);

    frame.render_widget(Paragraph::new(Line::from(rule)), chunks[3]);
    frame.render_widget(Paragraph::new(footer_line(len, height, offset)), chunks[4]);
  }
}

// ── Errors view ───────────────────────────────────────────────────────────

fn render_errors(frame: &mut Frame, area: Rect, state: &mut AppState) {
  let rows = visible_rows(state);
  let block = styled_block(
    format!(" errors · {} of {} ", rows.len(), state.total),
    Color::Red,
  );
  let inner = block.inner(area);
  frame.render_widget(block, area);

  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Length(1),
      Constraint::Length(1),
      Constraint::Min(0),
      Constraint::Length(1),
      Constraint::Length(1),
    ])
    .split(inner);

  let cols = grid::error_columns(inner.width);
  let rule = Span::styled("─".repeat(inner.width as usize), dim());

  let header = Line::from(vec![
    Span::styled(grid::fit("#", cols.index as usize), dim()),
    Span::styled(grid::fit("rom", cols.name as usize), dim()),
    Span::styled(grid::fit("id", cols.id as usize), dim()),
    Span::styled(grid::fit("pkg", cols.pkg as usize), dim()),
    Span::styled(grid::fit("rom", cols.rom as usize), dim()),
    Span::styled(grid::fit("cause", cols.cause as usize), dim()),
    Span::styled("attempts".to_string(), dim()),
  ]);
  frame.render_widget(Paragraph::new(header), chunks[0]);
  frame.render_widget(Paragraph::new(Line::from(rule.clone())), chunks[1]);

  if rows.is_empty() {
    frame.render_widget(
      Paragraph::new(Line::from(Span::styled(
        "no failures — press esc to go back".to_string(),
        dim().add_modifier(Modifier::ITALIC),
      ))),
      chunks[2],
    );
    return;
  }

  let height = chunks[2].height as usize;
  let cursor = rows.iter().position(|&i| i == state.selected).unwrap_or(0);
  state.scroll = grid::clamp_scroll(state.scroll, rows.len(), height, cursor);
  let spinner = SPINNER_FRAMES[state.tick % SPINNER_FRAMES.len()];

  let items: Vec<ListItem> = rows
    .iter()
    .enumerate()
    .skip(state.scroll)
    .take(height)
    .map(|(pos, &row)| {
      let entry = &state.roms[row];
      let selected = pos == cursor;
      let bg = |style: Style| {
        if selected {
          style.bg(SELECTED_BG)
        } else {
          style
        }
      };
      let name = if selected {
        format!("▌{}", entry.label)
      } else {
        entry.label.clone()
      };
      ListItem::new(Line::from(vec![
        Span::styled(
          grid::fit(&(row + 1).to_string(), cols.index as usize),
          bg(dim()),
        ),
        Span::styled(
          grid::fit(&name, cols.name as usize),
          bg(Style::default().fg(Color::Red)),
        ),
        cell_span(entry.id, cols.id as usize, spinner, selected),
        cell_span(entry.pkg, cols.pkg as usize, spinner, selected),
        cell_span(entry.rom, cols.rom as usize, spinner, selected),
        Span::styled(
          grid::fit(
            entry.error.as_deref().unwrap_or("unknown cause"),
            cols.cause as usize,
          ),
          bg(Style::default()),
        ),
        Span::styled(
          grid::fit(&entry.attempts.to_string(), cols.attempts as usize),
          bg(dim()),
        ),
      ]))
    })
    .collect();
  frame.render_widget(List::new(items), chunks[2]);

  frame.render_widget(Paragraph::new(Line::from(rule)), chunks[3]);
  let causes: Vec<String> = rows
    .iter()
    .filter_map(|&i| state.roms[i].error.clone())
    .collect();
  frame.render_widget(
    Paragraph::new(Line::from(Span::styled(errors::tally(&causes), dim()))),
    chunks[4],
  );
}

/// The three lines unfolded under the selected row.
///
/// The first is always the file it came from. The other two follow what the ROM is
/// doing: a failure shows its cause and a ROM waiting on the modal shows what the name
/// search found, because those are the two states the user opened the row to act on.
fn detail_lines(entry: &RomEntry, width: usize) -> Vec<Line<'static>> {
  let bg = Style::default().bg(SELECTED_BG);
  let label = |text: &str| Span::styled(grid::fit(text, 12), dim().bg(SELECTED_BG));
  let pad = Span::styled(grid::fit("", 6), bg);
  // What is left once the 6-cell indent and the 12-cell label are taken.
  let rest = width.saturating_sub(18);

  let size = entry
    .size
    .map(grid::format_bytes)
    .unwrap_or_else(|| "—".to_string());

  let first = Line::from(vec![
    pad.clone(),
    label("file"),
    Span::styled(grid::fit(&entry.file_name, 34), bg),
    Span::styled(grid::fit(&size, 14), dim().bg(SELECTED_BG)),
    Span::styled(
      grid::truncate(&format!("source {}", entry.source), rest.saturating_sub(48)),
      dim().bg(SELECTED_BG),
    ),
  ]);

  let second = Line::from(vec![
    pad.clone(),
    label("sha1"),
    Span::styled(
      grid::fit(entry.sha1.as_deref().unwrap_or("—"), 48),
      dim().bg(SELECTED_BG),
    ),
    Span::styled(
      grid::truncate(
        if entry.id == Cell::Waiting {
          "no hash match on screenscraper"
        } else {
          ""
        },
        rest.saturating_sub(48),
      ),
      dim().bg(SELECTED_BG),
    ),
  ]);

  let (third_label, third_value, third_note) = if let Some(cause) = &entry.error {
    ("error", cause.clone(), "")
  } else if entry.id == Cell::Waiting {
    (
      "search",
      match entry.candidates {
        Some(n) => format!("{} candidate(s) by name", n),
        None => "searching by name".to_string(),
      },
      "enter opens the modal",
    )
  } else {
    ("output", format!("./{}/", dir_name(&entry.label)), "")
  };

  let third = Line::from(vec![
    pad,
    label(third_label),
    Span::styled(grid::fit(&third_value, 48), bg),
    Span::styled(
      grid::truncate(third_note, rest.saturating_sub(48)),
      dim().bg(SELECTED_BG),
    ),
  ]);

  vec![first, second, third]
}

/// The output directory rompom writes for a ROM: its logical name without extension.
fn dir_name(label: &str) -> String {
  match label.rsplit_once('.') {
    Some((stem, _)) if !stem.is_empty() => stem.to_string(),
    _ => label.to_string(),
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

fn row_line(
  index: usize,
  entry: &RomEntry,
  cols: &Columns,
  spinner: &str,
  selected: bool,
) -> Line<'static> {
  // The cursor takes the first cell of the name column rather than a column of its own:
  // every other column would shift by one as the selection moved.
  let name = if selected {
    format!("▌{}", entry.label)
  } else {
    entry.label.clone()
  };
  let bg = |style: Style| {
    if selected {
      style.bg(SELECTED_BG)
    } else {
      style
    }
  };

  let mut spans = vec![
    Span::styled(
      grid::fit(&(index + 1).to_string(), cols.index as usize),
      bg(dim()),
    ),
    Span::styled(grid::fit(&name, cols.name as usize), bg(name_style(entry))),
    cell_span(entry.id, cols.id as usize, spinner, selected),
    cell_span(entry.pkg, cols.pkg as usize, spinner, selected),
    cell_span(entry.rom, cols.rom as usize, spinner, selected),
  ];

  for i in 0..MEDIA_COUNT {
    spans.push(dot_span(entry.media[i], cols.media_cell as usize, selected));
  }

  let time = entry
    .elapsed()
    .map(grid::format_elapsed)
    .unwrap_or_else(|| "—".to_string());
  spans.push(Span::styled(
    grid::fit(&time, cols.time as usize),
    bg(dim()),
  ));
  spans.push(Span::styled(
    grid::fit(&entry.status, cols.status as usize),
    bg(status_style(entry)),
  ));

  Line::from(spans)
}

fn cell_span(cell: Cell, width: usize, spinner: &str, selected: bool) -> Span<'static> {
  let (glyph, color) = match cell {
    Cell::Todo => ("·", Color::DarkGray),
    Cell::Running => (spinner, Color::Cyan),
    Cell::Waiting => (spinner, Color::Yellow),
    Cell::Done => ("✓", Color::Green),
    Cell::Unchanged => ("=", Color::DarkGray),
    Cell::Failed => ("✗", Color::Red),
  };
  let style = Style::default().fg(color);
  Span::styled(
    grid::fit(glyph, width),
    if selected {
      style.bg(SELECTED_BG)
    } else {
      style
    },
  )
}

fn dot_span(dot: Dot, width: usize, selected: bool) -> Span<'static> {
  let (glyph, color) = match dot {
    Dot::Todo => ("·", Color::DarkGray),
    Dot::Running => ("◐", Color::Green),
    Dot::Fresh => ("●", Color::Green),
    Dot::Unchanged => ("●", Color::DarkGray),
    Dot::Missing => ("○", Color::Red),
  };
  let style = Style::default().fg(color);
  Span::styled(
    grid::fit(glyph, width),
    if selected {
      style.bg(SELECTED_BG)
    } else {
      style
    },
  )
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

// ── To-identify view ──────────────────────────────────────────────────────

/// The ROMs blocked on the user, and how long each has been waiting.
///
/// The last column is the candidate ScreenScraper ranked first, not a match score:
/// `jeuRecherche` returns its list "sorted by probability" and no percentage at all, so
/// the name is the only real thing to put there — and it is often enough to decide
/// without opening the modal.
fn render_unidentified(frame: &mut Frame, area: Rect, state: &mut AppState) {
  let rows = visible_rows(state);
  let block = styled_block(
    format!(" to identify · {} waiting ", rows.len()),
    Color::Yellow,
  );
  let inner = block.inner(area);
  frame.render_widget(block, area);

  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Length(1),
      Constraint::Length(1),
      Constraint::Min(0),
    ])
    .split(inner);

  let (w_index, w_file, w_wait, w_count) = (6usize, 38usize, 12usize, 12usize);
  let w_best = (inner.width as usize).saturating_sub(w_index + w_file + w_wait + w_count);

  frame.render_widget(
    Paragraph::new(Line::from(vec![
      Span::styled(grid::fit("#", w_index), dim()),
      Span::styled(grid::fit("file", w_file), dim()),
      Span::styled(grid::fit("waiting", w_wait), dim()),
      Span::styled(grid::fit("candidates", w_count), dim()),
      Span::styled("best match".to_string(), dim()),
    ])),
    chunks[0],
  );
  frame.render_widget(
    Paragraph::new(Line::from(Span::styled(
      "─".repeat(inner.width as usize),
      dim(),
    ))),
    chunks[1],
  );

  if rows.is_empty() {
    frame.render_widget(
      Paragraph::new(Line::from(Span::styled(
        "nothing waiting — press esc to go back".to_string(),
        dim().add_modifier(Modifier::ITALIC),
      ))),
      chunks[2],
    );
    return;
  }

  let height = chunks[2].height as usize;
  let cursor = rows.iter().position(|&i| i == state.selected).unwrap_or(0);
  state.scroll = grid::clamp_scroll(state.scroll, rows.len(), height, cursor);

  let items: Vec<ListItem> = rows
    .iter()
    .enumerate()
    .skip(state.scroll)
    .take(height)
    .map(|(pos, &row)| {
      let entry = &state.roms[row];
      let selected = pos == cursor;
      let bg = |style: Style| {
        if selected {
          style.bg(WAITING_BG)
        } else {
          style
        }
      };
      let file = if selected {
        format!("▌{}", entry.file_name)
      } else {
        entry.file_name.clone()
      };
      let waiting = entry
        .waiting_since
        .map(|t| grid::format_elapsed(t.elapsed()))
        .unwrap_or_else(|| "—".to_string());
      let (count, count_style) = match entry.candidates {
        Some(0) | None => ("none".to_string(), Style::default().fg(Color::Red)),
        Some(n) => (n.to_string(), Style::default()),
      };

      ListItem::new(Line::from(vec![
        Span::styled(grid::fit(&(row + 1).to_string(), w_index), bg(dim())),
        Span::styled(
          grid::fit(&file, w_file),
          bg(Style::default().fg(Color::Yellow)),
        ),
        Span::styled(grid::fit(&waiting, w_wait), bg(dim())),
        Span::styled(grid::fit(&count, w_count), bg(count_style)),
        Span::styled(
          grid::fit(entry.best_candidate.as_deref().unwrap_or("—"), w_best),
          bg(Style::default()),
        ),
      ]))
    })
    .collect();
  frame.render_widget(List::new(items), chunks[2]);
}

fn keys(pairs: &[(&str, String)]) -> Paragraph<'static> {
  let mut spans = Vec::new();
  for (key, what) in pairs {
    spans.push(Span::styled(
      key.to_string(),
      Style::default().fg(Color::Cyan),
    ));
    spans.push(Span::styled(what.clone(), dim()));
  }
  Paragraph::new(Line::from(spans))
}

fn help_line(state: &AppState) -> Paragraph<'static> {
  let failed = state.roms.iter().filter(|r| r.failed()).count();
  let waiting = state.roms.iter().filter(|r| r.id == Cell::Waiting).count();
  let filter = match state.filter {
    Filter::All => " filter: all  ",
    Filter::Active => " filter: active  ",
    Filter::Errors => " filter: errors  ",
    Filter::Unidentified => " filter: to identify  ",
  };
  keys(&[
    ("↑↓", " select  ".to_string()),
    ("g/G", " top/bottom  ".to_string()),
    ("f", filter.to_string()),
    ("e", format!(" errors ({})  ", failed)),
    ("m", format!(" to identify ({})  ", waiting)),
    ("ctrl-c", " stop".to_string()),
  ])
}

fn unidentified_help_line() -> Paragraph<'static> {
  keys(&[
    ("↑↓", " select  ".to_string()),
    ("enter", " identify  ".to_string()),
    ("s", " skip  ".to_string()),
    ("esc", " back  ".to_string()),
    ("ctrl-c", " stop".to_string()),
  ])
}

fn errors_help_line() -> Paragraph<'static> {
  keys(&[
    ("↑↓", " select  ".to_string()),
    ("w", " write <system>.errors.log  ".to_string()),
    ("esc", " back  ".to_string()),
    ("ctrl-c", " stop".to_string()),
  ])
}

/// The hint line, unless something has just happened that is worth saying instead.
fn hints_or_notice(state: &AppState, hints: Paragraph<'static>) -> Paragraph<'static> {
  match &state.notice {
    Some(msg) => Paragraph::new(Line::from(Span::styled(
      msg.clone(),
      Style::default().fg(Color::Yellow),
    ))),
    None => hints,
  }
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
