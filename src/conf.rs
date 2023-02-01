use std::{
   io,
   fs,
   path::PathBuf
};

use snafu::{
   ResultExt,
   Snafu,
   Backtrace
};

#[derive(Deserialize)]
pub struct Auth {
   pub login:         String,
   pub password:      String
}

#[derive(Deserialize)]
pub struct ScreenScraper {
   pub dev:           Auth,
   pub user:          Auth
}

#[derive(Deserialize, Clone)]
pub struct System {
   pub name:          String,
   pub id:            u32,
   pub basename:      String,
   pub depends:       Option<String>,
   pub dir:           String,
   pub checksum:      Option<String>
}

#[derive(Deserialize)]
pub struct Conf {
   pub screenscraper: ScreenScraper,
   pub systems:       Vec<System>
}

#[derive(Deserialize)]
pub struct Reference {
   pub gameid:   u64,
   pub gamerom:  String,
   pub systemid: u32
}

#[derive(Debug, Snafu)]
pub enum Error {
   ReadConfiguration {
      source: io::Error,
      backtrace: Backtrace,
      path: PathBuf
   },
   ParseConfiguration {
      source: serde_yaml::Error
   },
}

type Result<T, E = Error> = std::result::Result<T, E>;

impl Conf {
   pub fn load(file: &String) -> Result<Conf> {
      let obj: Conf;
      let data = fs::read_to_string(file.clone()).context(ReadConfigurationSnafu { path: file })?;

      obj = serde_yaml::from_str(data.as_str()).context(ParseConfigurationSnafu)?;

      Ok(obj)
   }

   pub fn system_find(&self, id: u32) -> System {
      for system in &self.systems {
         if system.id == id {
            return system.clone()
         }
      }

      System {
         name:     "unknown".to_string(),
         id:       0,
         basename: "unknown-rom-".to_string(),
         depends:  None,
         checksum: None,
         dir:      "unknown".to_string()
      }
   }
}

impl System {
   pub fn checksum_disabled(&self) -> bool {
      if let Some(ref x) = self.checksum {
         match x.as_str() {
            "disable" => { return true;  },
            _         => { return false; }
         }
      }
      false
   }
}

impl Reference {
   pub fn load(file: &String) -> Result<Reference> {
      let obj: Reference;
      let data = fs::read_to_string(file.clone()).context(ReadConfigurationSnafu { path: file })?;
      
      obj = serde_yaml::from_str(data.as_str()).context(ParseConfigurationSnafu)?;
      Ok(obj)
   }
}
