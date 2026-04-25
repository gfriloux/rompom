use std::{fs, io};

use crossterm::{
  event::{self, Event, KeyCode, KeyModifiers},
  execute,
  terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
  backend::CrosstermBackend,
  layout::{Constraint, Direction, Layout},
  style::{Color, Modifier, Style},
  widgets::{Block, Borders, List, ListItem, Paragraph},
  Terminal,
};
use snafu::ResultExt;

use super::{
  Conf, ConfRaw, ParseConfigurationSnafu, ReadConfigurationSnafu, Result, WriteConfigurationSnafu,
  SUPPORTED_LANGS,
};

pub(super) fn order_languages_tui() -> Option<Vec<String>> {
  enable_raw_mode().ok()?;
  let mut stdout = io::stdout();
  execute!(stdout, EnterAlternateScreen).ok()?;
  let backend = CrosstermBackend::new(stdout);
  let mut terminal = Terminal::new(backend).ok()?;

  // All languages are always present — user just sets their priority order
  let mut order: Vec<usize> = (0..SUPPORTED_LANGS.len()).collect();
  let mut cursor: usize = 0;
  let mut result = None;

  loop {
    let cursor_c = cursor;
    let order_c = order.clone();

    terminal
      .draw(|f| {
        let area = f.area();
        let block = Block::default()
          .title(" rompom — Language priority ")
          .borders(Borders::ALL);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
          .direction(Direction::Vertical)
          .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
          ])
          .split(inner);

        let hint =
          Paragraph::new("↑↓ navigate · +/- move item up/down · Enter confirm · Esc cancel");
        f.render_widget(hint, chunks[0]);

        let items: Vec<ListItem> = order_c
          .iter()
          .enumerate()
          .map(|(pos, &lang_idx)| {
            let (code, name) = SUPPORTED_LANGS[lang_idx];
            let style = if pos == cursor_c {
              Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
            } else {
              Style::default()
            };
            ListItem::new(format!("{:2}  {} — {}", pos + 1, code, name)).style(style)
          })
          .collect();

        f.render_widget(List::new(items), chunks[1]);

        let priority_text: Vec<&str> = order_c.iter().map(|&i| SUPPORTED_LANGS[i].0).collect();
        f.render_widget(
          Paragraph::new(format!("Priority: {}", priority_text.join(" → ")))
            .style(Style::default().fg(Color::Cyan)),
          chunks[2],
        );
      })
      .ok();

    if let Ok(Event::Key(key)) = event::read() {
      match (key.code, key.modifiers) {
        (KeyCode::Up, KeyModifiers::NONE) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
          cursor = cursor.saturating_sub(1);
        }
        (KeyCode::Down, KeyModifiers::NONE) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
          if cursor < order.len() - 1 {
            cursor += 1;
          }
        }
        (KeyCode::Char('+'), _) => {
          if cursor > 0 {
            order.swap(cursor, cursor - 1);
            cursor -= 1;
          }
        }
        (KeyCode::Char('-'), _) => {
          if cursor < order.len() - 1 {
            order.swap(cursor, cursor + 1);
            cursor += 1;
          }
        }
        (KeyCode::Enter, _) => {
          result = Some(
            order
              .iter()
              .map(|&i| SUPPORTED_LANGS[i].0.to_string())
              .collect(),
          );
          break;
        }
        (KeyCode::Esc, _) | (KeyCode::Char('q'), KeyModifiers::NONE) => {
          break;
        }
        _ => {}
      }
    }
  }

  let _ = disable_raw_mode();
  let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
  result
}

impl Conf {
  pub fn update(file: &str) -> Result<()> {
    let data = fs::read_to_string(file).context(ReadConfigurationSnafu { path: file })?;
    let raw: ConfRaw = serde_yaml::from_str(data.as_str()).context(ParseConfigurationSnafu)?;

    let lang_missing = raw.lang.as_ref().is_none_or(|l| l.is_empty());
    let ia_items_present = raw.systems.iter().any(|s| s.ia_items.is_some());

    if !lang_missing && !ia_items_present {
      println!("Configuration is already up to date.");
      return Ok(());
    }

    // Migration 1 : lang manquant → TUI de sélection
    let data = if lang_missing {
      let chosen = match order_languages_tui() {
        Some(langs) => langs,
        None => {
          eprintln!("No language selected. Aborting.");
          std::process::exit(1);
        }
      };

      let lang_yaml = chosen
        .iter()
        .map(|c| format!("  - {}", c))
        .collect::<Vec<_>>()
        .join("\n");
      let lang_block = format!("lang:\n{}\n\n", lang_yaml);

      let updated = if data.contains("\nlang:") || data.starts_with("lang:") {
        let re_start = data.find("lang:").unwrap();
        let rest = &data[re_start + 5..];
        let re_end = rest
          .find("\n\n")
          .or_else(|| rest.find("\nsystems:"))
          .map(|i| re_start + 5 + i)
          .unwrap_or(data.len());
        format!(
          "{}{}{}",
          &data[..re_start],
          lang_block,
          &data[re_end..].trim_start_matches('\n')
        )
      } else {
        data.replacen("systems:", &format!("{}systems:", lang_block), 1)
      };

      fs::write(file, &updated).context(WriteConfigurationSnafu { path: file })?;
      println!(
        "Configuration updated: {} (lang: {})",
        file,
        chosen.join(", ")
      );
      updated
    } else {
      data
    };

    // Migration 2 : ia_items → source.internet_archive
    if ia_items_present {
      let mut value: serde_yaml::Value =
        serde_yaml::from_str(&data).context(ParseConfigurationSnafu)?;

      if let Some(systems) = value.get_mut("systems").and_then(|s| s.as_sequence_mut()) {
        for system in systems.iter_mut() {
          let ia_items = match system.get("ia_items") {
            Some(v) => v.clone(),
            None => continue,
          };

          let new_items: Vec<serde_yaml::Value> = ia_items
            .as_sequence()
            .unwrap_or(&vec![])
            .iter()
            .map(|item| {
              let mut new_item = serde_yaml::Mapping::new();
              new_item.insert(
                serde_yaml::Value::String("item".to_string()),
                item["item"].clone(),
              );
              let filter = item["filter"].as_str().unwrap_or("*");
              new_item.insert(
                serde_yaml::Value::String("filter".to_string()),
                serde_yaml::Value::Sequence(vec![serde_yaml::Value::String(filter.to_string())]),
              );
              serde_yaml::Value::Mapping(new_item)
            })
            .collect();

          let mut source_map = serde_yaml::Mapping::new();
          source_map.insert(
            serde_yaml::Value::String("internet_archive".to_string()),
            serde_yaml::Value::Sequence(new_items),
          );

          if let Some(map) = system.as_mapping_mut() {
            map.remove("ia_items");
            map.insert(
              serde_yaml::Value::String("source".to_string()),
              serde_yaml::Value::Mapping(source_map),
            );
          }
        }
      }

      let updated = serde_yaml::to_string(&value).context(ParseConfigurationSnafu)?;
      fs::write(file, updated).context(WriteConfigurationSnafu { path: file })?;
      println!("Configuration updated: ia_items migrated to source.internet_archive");
    }

    Ok(())
  }
}
