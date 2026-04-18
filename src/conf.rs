use serde_derive::Deserialize;
use std::{fs, io, path::PathBuf};

use snafu::{Backtrace, ResultExt, Snafu};

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
pub struct Conf {
  pub screenscraper: ScreenScraper,
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
}

type Result<T, E = Error> = std::result::Result<T, E>;

impl Conf {
  pub fn load(file: &String) -> Result<Conf> {
    let data = fs::read_to_string(file.clone()).context(ReadConfigurationSnafu { path: file })?;
    let obj: Conf = serde_yaml::from_str(data.as_str()).context(ParseConfigurationSnafu)?;
    Ok(obj)
  }
  pub fn system_find(&self, name: &str) -> System {
    for system in &self.systems {
      if system.name.eq(name) {
        return system.clone();
      }
    }

    System {
      name: "unknown".to_string(),
      id: 0,
      basename: "unknown-rom-".to_string(),
      depends: None,
      dir: "unknown".to_string(),
      ia_items: None,
    }
  }
}
