use ratatui::{
  layout::{Constraint, Direction, Layout, Rect},
  style::{Color, Modifier, Style},
  text::{Line, Span},
  widgets::{Block, BorderType, Borders, Gauge, List, ListItem, Paragraph},
  Frame,
};

use super::{
  AppState, CompletedEntry, ModalDisplayState, ModalMode, PanelDef, RomEntry, MEDIA_ICONS, PANELS,
  PANEL_HEIGHT, SPINNER_FRAMES,
};

// ── Top-level render ────────────────────────────────────────────────────────

pub(super) fn render(frame: &mut Frame, state: &AppState) {
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

// ── Shared widget helpers ─────────────────────────────────────────────────

fn styled_block(title: String, color: Color) -> Block<'static> {
  Block::default()
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .border_style(Style::default().fg(color))
    .title(title)
    .title_style(Style::default().fg(color).add_modifier(Modifier::BOLD))
}

// ── Completed panel ───────────────────────────────────────────────────────

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
    state
      .completed
      .iter()
      .map(|entry| completed_item(entry, chunks[1].width as usize))
      .collect()
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

/// Fits a failure cause into what is left of the line.
///
/// The Completed panel is a fixed-width list: an untruncated cause would run past the
/// border and take the ROM's name with it. The full text is kept for `Summary::print()`,
/// which has a whole terminal line to itself.
fn truncate_cause(cause: &str, available: usize) -> String {
  let cause = cause.replace('\n', " ");
  match available {
    0 => String::new(),
    1 => "…".to_string(),
    _ if cause.chars().count() <= available => cause,
    _ => {
      let kept: String = cause.chars().take(available - 1).collect();
      format!("{}…", kept.trim_end())
    }
  }
}

fn completed_item(entry: &CompletedEntry, width: usize) -> ListItem<'static> {
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

  // A failed ROM shows its cause instead of its media icons. The icons describe a
  // package that was never finished, and both would not fit on the line anyway — the
  // cause is the one thing the user can act on.
  if !entry.success {
    if let Some(ref cause) = entry.error {
      let used = check.chars().count() + entry.label.chars().count() + 2;
      let cause = truncate_cause(cause, width.saturating_sub(used));
      if !cause.is_empty() {
        spans.push(Span::styled(cause, Style::default().fg(Color::DarkGray)));
      }
    }
    return ListItem::new(Line::from(spans));
  }

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

// ── Active panels ─────────────────────────────────────────────────────────

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

#[cfg(test)]
mod tests {
  use super::*;

  /// A cause that fits is shown as is — no gratuitous ellipsis.
  #[test]
  fn a_short_cause_is_left_alone() {
    assert_eq!(truncate_cause("host unreachable", 40), "host unreachable");
    assert_eq!(truncate_cause("exact", 5), "exact");
  }

  /// Past the panel width the line would run over the border and push the ROM name out
  /// of view, so the cause is cut and marked as cut.
  #[test]
  fn a_long_cause_is_cut_and_says_so() {
    let cause = "too many unrecognised ROMs today — ScreenScraper says come back tomorrow";
    let cut = truncate_cause(cause, 20);
    assert_eq!(cut.chars().count(), 20);
    assert!(cut.ends_with('…'));
    assert!(cause.starts_with(cut.trim_end_matches('…')));
  }

  /// A narrow terminal must not panic on the arithmetic, and must not emit a bare
  /// dangling ellipsis wider than the space it was given.
  #[test]
  fn a_narrow_panel_does_not_overflow() {
    assert_eq!(truncate_cause("anything", 0), "");
    assert_eq!(truncate_cause("anything", 1), "…");
    assert_eq!(truncate_cause("anything", 2).chars().count(), 2);
  }

  /// When the cut lands just after a space, keeping it renders as a gap floating before
  /// the ellipsis. Cutting mid-word is left alone — the ellipsis says enough.
  #[test]
  fn the_cut_does_not_leave_a_dangling_space() {
    assert_eq!(truncate_cause("could not reach it", 11), "could not…");
    assert_eq!(truncate_cause("could not reach it", 12), "could not r…");
  }

  /// Causes reach the panel from `StepStatus::Failed`, and a panic message carries the
  /// panic location, which contains no newline — but a download error may well.
  #[test]
  fn newlines_would_break_the_list_layout() {
    assert_eq!(truncate_cause("first\nsecond", 40), "first second");
  }
}
