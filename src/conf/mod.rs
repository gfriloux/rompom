mod update;

use serde_derive::Deserialize;
use std::{fs, io, path::PathBuf};

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

  pub fn find_system(&self, name: &str) -> Option<System> {
    self.systems.iter().find(|s| s.name == name).cloned()
  }
}
