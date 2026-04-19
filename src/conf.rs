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

/// Ancien format ia_items — utilisé uniquement pour détecter et migrer la conf
#[derive(Deserialize, Clone, Debug)]
#[allow(dead_code)]
struct ItemOld {
  item: String,
  filter: String,
}

/// Entrée Internet Archive (nouveau format, filter en liste)
#[derive(Deserialize, Clone, Debug)]
pub struct IaItem {
  pub item: String,
  pub filter: Vec<String>,
}

#[derive(Deserialize, Clone, Debug)]
#[allow(dead_code)]
pub struct FolderSource {
  pub path: String,
  pub filter: Vec<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub enum Source {
  #[serde(rename = "internet_archive")]
  InternetArchive(Vec<IaItem>),
  #[serde(rename = "folder")]
  #[allow(dead_code)]
  Folder(FolderSource),
}

/// Système brut — accepte l'ancien champ ia_items pour détecter la migration nécessaire
#[derive(Deserialize, Clone, Debug)]
struct SystemRaw {
  pub name: String,
  pub id: u32,
  pub basename: String,
  pub depends: Option<String>,
  pub dir: String,
  pub ia_items: Option<Vec<ItemOld>>,
  #[serde(default)]
  #[serde(with = "serde_yaml::with::singleton_map_recursive")]
  pub source: Option<Source>,
}

#[derive(Clone, Debug)]
pub struct System {
  pub name: String,
  pub id: u32,
  pub basename: String,
  pub depends: Option<String>,
  pub dir: String,
  pub source: Option<Source>,
}

#[derive(Deserialize, Debug)]
struct ConfRaw {
  pub screenscraper: ScreenScraper,
  pub lang: Option<Vec<String>>,
  pub systems: Vec<SystemRaw>,
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

    if raw.systems.iter().any(|s| s.ia_items.is_some()) {
      return Err(Error::ConfigNeedsUpdate);
    }

    let systems = raw
      .systems
      .into_iter()
      .map(|s| System {
        name: s.name,
        id: s.id,
        basename: s.basename,
        depends: s.depends,
        dir: s.dir,
        source: s.source,
      })
      .collect();

    Ok(Conf {
      screenscraper: raw.screenscraper,
      lang,
      systems,
    })
  }

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

  pub fn find_system(&self, name: &str) -> Option<System> {
    self.systems.iter().find(|s| s.name == name).cloned()
  }
}
