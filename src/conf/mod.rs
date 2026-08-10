mod update;

use serde::Deserialize;
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

// Without a `display`, snafu falls back to the variant name: a missing config file
// reported itself as "ReadConfiguration", dropping both the path it tried and the
// reason. serde_yaml puts the line and column in its own Display, so `{source}` is
// what makes a YAML mistake findable.
#[derive(Debug, Snafu)]
pub enum Error {
  #[snafu(display("cannot read the configuration file {}: {}", path.display(), source))]
  ReadConfiguration {
    source: io::Error,
    backtrace: Backtrace,
    path: PathBuf,
  },
  #[snafu(display("invalid configuration in {}: {}", path.display(), source))]
  ParseConfiguration {
    source: serde_yaml::Error,
    path: PathBuf,
  },
  #[snafu(display("cannot write the configuration file {}: {}", path.display(), source))]
  WriteConfiguration {
    source: io::Error,
    backtrace: Backtrace,
    path: PathBuf,
  },
  #[snafu(display("cannot serialise the updated configuration for {}: {}", path.display(), source))]
  SerializeConfiguration {
    source: serde_yaml::Error,
    path: PathBuf,
  },
  #[snafu(display("Configuration needs to be updated. Run: rompom --update-config"))]
  ConfigNeedsUpdate,
}

type Result<T, E = Error> = std::result::Result<T, E>;

impl Conf {
  pub fn load(file: &String) -> Result<Conf> {
    let data = fs::read_to_string(file.clone()).context(ReadConfigurationSnafu { path: file })?;
    let raw: ConfRaw =
      serde_yaml::from_str(data.as_str()).context(ParseConfigurationSnafu { path: file })?;

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

#[cfg(test)]
mod tests {
  use super::*;

  /// Without a `display` attribute snafu prints the variant name, so a missing config
  /// file reported "ReadConfiguration" and left the user to guess which path rompom had
  /// tried — `$XDG_CONFIG_HOME` makes that a real question.
  #[test]
  fn a_missing_file_names_the_path_it_tried() {
    let path = "/nonexistent/rompom-does-not-live-here.yml".to_string();
    let message = Conf::load(&path).unwrap_err().to_string();

    assert!(message.contains(&path), "got: {}", message);
    assert!(!message.contains("ReadConfiguration"), "got: {}", message);
  }

  /// The whole value of `{source}` here: serde_yaml carries the line and column, and a
  /// YAML mistake is otherwise a needle in a config that lists every system.
  #[test]
  fn a_yaml_mistake_points_at_the_line() {
    let source = serde_yaml::from_str::<ConfRaw>("screenscraper:\n  dev: [\n").unwrap_err();
    let error = Error::ParseConfiguration {
      source,
      path: PathBuf::from("/home/user/.config/rompom.yml"),
    };
    let message = error.to_string();

    assert!(
      message.contains("/home/user/.config/rompom.yml"),
      "got: {}",
      message
    );
    assert!(message.contains("line"), "got: {}", message);
  }

  /// Reading, parsing and writing used to be told apart only by a variant name nobody
  /// saw. They are three different things to go and fix.
  #[test]
  fn each_failure_reads_differently() {
    let path = PathBuf::from("/home/user/.config/rompom.yml");
    let parse = Error::ParseConfiguration {
      source: serde_yaml::from_str::<ConfRaw>("[").unwrap_err(),
      path: path.clone(),
    };
    let serialize = Error::SerializeConfiguration {
      source: serde_yaml::from_str::<ConfRaw>("[").unwrap_err(),
      path,
    };

    assert_ne!(parse.to_string(), serialize.to_string());
    assert!(Error::ConfigNeedsUpdate
      .to_string()
      .contains("--update-config"));
  }
}
