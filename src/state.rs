use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RomStateEntry {
  pub ss_game_id: Option<String>,
  pub rom_sha1: String,
  pub medias: HashMap<String, Option<String>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SystemState {
  pub roms: HashMap<String, RomStateEntry>,
}

impl SystemState {
  pub fn load(path: &str) -> Self {
    std::fs::read_to_string(path)
      .ok()
      .and_then(|s| serde_yaml::from_str(&s).ok())
      .unwrap_or_default()
  }

  pub fn save(&self, path: &str) {
    let yaml = serde_yaml::to_string(self).unwrap();
    std::fs::write(path, yaml).unwrap();
  }

  pub fn insert(&mut self, filename: String, entry: RomStateEntry) {
    self.roms.insert(filename, entry);
  }
}
