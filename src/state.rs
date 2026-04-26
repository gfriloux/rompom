use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RomStateEntry {
  pub ss_game_id: Option<String>,
  pub rom_sha1: String,
  /// Timestamp de modification du fichier ROM (secondes Unix). 0 = non renseigné.
  /// Utilisé pour le fast-skip SHA1 sur les sources folder.
  #[serde(default)]
  pub rom_mtime: u64,
  /// Taille du fichier ROM en octets. 0 = non renseigné.
  #[serde(default)]
  pub rom_size: u64,
  pub medias: HashMap<String, Option<String>>,
  /// SHA-1 hashes for extra discs (disc 2, 3, …).  Empty for single-disc ROMs.
  #[serde(default)]
  pub extra_disc_sha1s: Vec<String>,
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

  /// Atomically persist the state using a write-rename pattern.
  ///
  /// If the destination file already exists:
  /// 1. Write new content to `<path>.tmp`.
  /// 2. Rename existing file to `<path>.old`.
  /// 3. Rename `<path>.tmp` to `<path>`.
  ///
  /// If the `tmp` write fails, the original file is untouched.
  /// If any subsequent rename fails, the method attempts to restore the
  /// original from `<path>.old` before returning the error.
  pub fn save_with_rotation(&self, path: &str) -> std::io::Result<()> {
    let yaml = serde_yaml::to_string(self).map_err(std::io::Error::other)?;

    if Path::new(path).exists() {
      let tmp = format!("{}.tmp", path);
      let old = format!("{}.old", path);

      // Write to tmp first — original is untouched if this fails.
      std::fs::write(&tmp, &yaml)?;

      // Move original out of the way.
      if let Err(e) = std::fs::rename(path, &old) {
        std::fs::remove_file(&tmp).ok();
        return Err(e);
      }

      // Promote tmp to final.
      if let Err(e) = std::fs::rename(&tmp, path) {
        // Try to restore original.
        std::fs::rename(&old, path).ok();
        return Err(e);
      }

      // Clean up the backup.
      std::fs::remove_file(&old).ok();
    } else {
      std::fs::write(path, yaml)?;
    }

    Ok(())
  }

  pub fn insert(&mut self, filename: String, entry: RomStateEntry) {
    self.roms.insert(filename, entry);
  }
}
