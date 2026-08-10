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
  /// Loads the saved state, and says so when it could not.
  ///
  /// The returned warning is the point. Failing silently to `default()` looks exactly
  /// like a first run: every ROM is re-hashed, re-scraped, re-downloaded and gets its
  /// `pkgver` bumped, and the only clue is that a quick run suddenly takes an hour.
  /// A missing file is a genuine first run and warns about nothing.
  pub fn load(path: &str) -> (Self, Option<String>) {
    let data = match std::fs::read_to_string(path) {
      Ok(data) => data,
      // Any other io error (permissions, a directory in the way) is worth naming.
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Self::default(), None),
      Err(e) => {
        return (
          Self::default(),
          Some(format!(
            "cannot read {} ({}) — every ROM will be treated as new",
            path, e
          )),
        )
      }
    };

    match serde_yaml::from_str(&data) {
      Ok(state) => (state, None),
      Err(e) => (
        Self::default(),
        Some(format!(
          "{} is unreadable ({}) — every ROM will be treated as new",
          path, e
        )),
      ),
    }
  }

  /// Serialises the state. Kept separate from writing so a caller can hold the mutex
  /// for the serialisation only, and do the file I/O without it.
  pub fn to_yaml(&self) -> std::io::Result<String> {
    serde_yaml::to_string(self).map_err(std::io::Error::other)
  }

  /// Atomically persist the state using a write-rename pattern.
  pub fn save_with_rotation(&self, path: &str) -> std::io::Result<()> {
    write_with_rotation(path, &self.to_yaml()?)
  }

  pub fn insert(&mut self, filename: String, entry: RomStateEntry) {
    self.roms.insert(filename, entry);
  }
}

/// Writes `content` to `path` without ever leaving a half-written file there.
///
/// If the destination already exists:
/// 1. Write the new content to `<path>.tmp`.
/// 2. Rename the existing file to `<path>.old`.
/// 3. Rename `<path>.tmp` onto `<path>`.
///
/// If the `tmp` write fails, the original is untouched. If a rename fails, the original
/// is restored from `<path>.old` before the error is returned. What the caller gets on a
/// crash is therefore one whole file or the other, never a truncated YAML that would
/// read as "nothing was ever done".
pub fn write_with_rotation(path: &str, content: &str) -> std::io::Result<()> {
  if Path::new(path).exists() {
    let tmp = format!("{}.tmp", path);
    let old = format!("{}.old", path);

    // Write to tmp first — original is untouched if this fails.
    std::fs::write(&tmp, content)?;

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
    std::fs::write(path, content)?;
  }

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  /// A unique directory per test, under the target dir so `cargo clean` takes it away.
  fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rompom-state-tests-{}", name));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
  }

  fn sample() -> SystemState {
    let mut state = SystemState::default();
    state.insert(
      "Sonic The Hedgehog (World).zip".to_string(),
      RomStateEntry {
        ss_game_id: Some("3".to_string()),
        rom_sha1: "abc123".to_string(),
        rom_mtime: 1_700_000_000,
        rom_size: 749_652,
        medias: HashMap::from([
          ("video".to_string(), Some("def456".to_string())),
          ("manual".to_string(), None),
        ]),
        extra_disc_sha1s: vec!["disc2sha".to_string()],
      },
    );
    state
  }

  /// Everything the fast paths depend on has to survive the round trip: lose `rom_mtime`
  /// and every folder ROM is re-hashed, lose `ss_game_id` and every ROM is re-scraped.
  #[test]
  fn a_state_survives_a_round_trip() {
    let dir = scratch("round-trip");
    let path = dir.join("system.state.yml");
    let path = path.to_str().unwrap();

    sample().save_with_rotation(path).unwrap();
    let (loaded, warning) = SystemState::load(path);

    assert!(warning.is_none());
    let entry = &loaded.roms["Sonic The Hedgehog (World).zip"];
    assert_eq!(entry.ss_game_id.as_deref(), Some("3"));
    assert_eq!(entry.rom_sha1, "abc123");
    assert_eq!(entry.rom_mtime, 1_700_000_000);
    assert_eq!(entry.rom_size, 749_652);
    assert_eq!(entry.medias["video"].as_deref(), Some("def456"));
    assert_eq!(entry.medias["manual"], None);
    assert_eq!(entry.extra_disc_sha1s, vec!["disc2sha".to_string()]);
  }

  /// A first run has no state file, and that is not a problem to report.
  #[test]
  fn a_missing_state_file_is_a_first_run() {
    let dir = scratch("missing");
    let path = dir.join("absent.state.yml");
    let (state, warning) = SystemState::load(path.to_str().unwrap());

    assert!(state.roms.is_empty());
    assert!(warning.is_none());
  }

  /// Silently starting from scratch looks exactly like a first run, except every ROM is
  /// re-downloaded and every pkgver bumped. The user gets told.
  #[test]
  fn an_unreadable_state_file_says_so() {
    let dir = scratch("corrupt");
    let path = dir.join("system.state.yml");
    std::fs::write(&path, "roms:\n  - this is not a mapping\n").unwrap();

    let (state, warning) = SystemState::load(path.to_str().unwrap());

    assert!(state.roms.is_empty());
    let warning = warning.expect("a corrupt state file must warn");
    assert!(warning.contains("system.state.yml"), "got: {}", warning);
    assert!(warning.contains("treated as new"), "got: {}", warning);
  }

  /// The rotation must leave the directory as it found it. A stray `.tmp` would be
  /// mistaken for a state file by nothing, but a leftover `.old` grows with every run.
  #[test]
  fn a_rewrite_leaves_no_debris() {
    let dir = scratch("debris");
    let path = dir.join("system.state.yml");
    let path = path.to_str().unwrap();

    write_with_rotation(path, "first: 1\n").unwrap();
    write_with_rotation(path, "second: 2\n").unwrap();

    assert_eq!(std::fs::read_to_string(path).unwrap(), "second: 2\n");
    let names: Vec<String> = std::fs::read_dir(&dir)
      .unwrap()
      .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
      .collect();
    assert_eq!(
      names,
      vec!["system.state.yml".to_string()],
      "got: {:?}",
      names
    );
  }

  /// The point of the write-rename: a reader either sees the whole previous file or the
  /// whole new one. Writing in place would expose a truncated YAML, which loads as an
  /// empty state — the exact failure this is meant to prevent.
  #[test]
  fn a_rewrite_is_never_seen_half_done() {
    let dir = scratch("atomic");
    let path = dir.join("system.state.yml");
    let path_str = path.to_str().unwrap();

    sample().save_with_rotation(path_str).unwrap();
    let before = std::fs::read_to_string(&path).unwrap();

    // The tmp file carries the new content; the destination still holds the old one
    // until the rename, which is the single instant the switch happens.
    std::fs::write(format!("{}.tmp", path_str), "half written").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);

    std::fs::remove_file(format!("{}.tmp", path_str)).unwrap();
    write_with_rotation(path_str, "roms: {}\n").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "roms: {}\n");
  }
}
