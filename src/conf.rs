use serde_derive::Deserialize;
use std::{fs, io, path::PathBuf};

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
use snafu::{Backtrace, ResultExt, Snafu};

// Source: https://www.screenscraper.fr — langues supportées pour les synopsis
pub const SUPPORTED_LANGS: &[(&str, &str)] = &[
  ("de", "Deutsch"),
  ("en", "English"),
  ("es", "Español"),
  ("fr", "Français"),
  ("it", "Italiano"),
  ("pt", "Português"),
];

#[derive(Deserialize, Debug)]
pub struct Auth {
  pub login: String,
  pub password: String,
}

#[derive(Deserialize, Debug)]
pub struct ScreenScraper {
  pub dev: Auth,
  pub user: Auth,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Item {
  pub item: String,
  pub filter: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct System {
  pub name: String,
  pub id: u32,
  pub basename: String,
  pub depends: Option<String>,
  pub dir: String,
  pub ia_items: Option<Vec<Item>>,
}

#[derive(Deserialize, Debug)]
struct ConfRaw {
  pub screenscraper: ScreenScraper,
  pub lang: Option<Vec<String>>,
  pub systems: Vec<System>,
}

#[derive(Debug)]
pub struct Conf {
  pub screenscraper: ScreenScraper,
  pub lang: Vec<String>,
  pub systems: Vec<System>,
}

#[derive(Debug, Snafu)]
pub enum Error {
  ReadConfiguration {
    source: io::Error,
    backtrace: Backtrace,
    path: PathBuf,
  },
  ParseConfiguration {
    source: serde_yaml::Error,
  },
  WriteConfiguration {
    source: io::Error,
    backtrace: Backtrace,
    path: PathBuf,
  },
  #[snafu(display("Configuration needs to be updated. Run: rompom --update-config"))]
  ConfigNeedsUpdate,
}

type Result<T, E = Error> = std::result::Result<T, E>;

fn order_languages_tui() -> Option<Vec<String>> {
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
  pub fn load(file: &String) -> Result<Conf> {
    let data = fs::read_to_string(file.clone()).context(ReadConfigurationSnafu { path: file })?;
    let raw: ConfRaw = serde_yaml::from_str(data.as_str()).context(ParseConfigurationSnafu)?;

    let lang = match raw.lang {
      Some(l) if !l.is_empty() => l,
      _ => return Err(Error::ConfigNeedsUpdate),
    };

    Ok(Conf {
      screenscraper: raw.screenscraper,
      lang,
      systems: raw.systems,
    })
  }

  pub fn update(file: &str) -> Result<()> {
    let data = fs::read_to_string(file).context(ReadConfigurationSnafu { path: file })?;
    let raw: ConfRaw = serde_yaml::from_str(data.as_str()).context(ParseConfigurationSnafu)?;

    let lang_missing = raw.lang.as_ref().is_none_or(|l| l.is_empty());

    if !lang_missing {
      println!("Configuration is already up to date.");
      return Ok(());
    }

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

    fs::write(file, updated).context(WriteConfigurationSnafu { path: file })?;
    println!(
      "Configuration updated: {} (lang: {})",
      file,
      chosen.join(", ")
    );
    Ok(())
  }

  pub fn find_system(&self, name: &str) -> Option<System> {
    self.systems.iter().find(|s| s.name == name).cloned()
  }
}
