use std::{path::PathBuf, sync::Arc};

use internet_archive::metadata::Metadata;

/// Source-specific data for an Internet Archive ROM.
pub struct IaSource {
  /// First HTTPS URL for this file from IA.
  pub rom_url: String,
  pub crc32: Option<String>,
  pub md5: Option<String>,
  /// SHA-1 from IA metadata (used directly; no `ComputeHashes` step).
  pub sha1: Option<String>,
  pub size: u64,
  /// Shared IA item metadata.
  pub metadata: Arc<Metadata>,
}

/// Source-specific data for a local folder ROM.
pub struct FolderSource {
  pub local_path: PathBuf,
}

/// Discriminated union of the two supported ROM sources.
pub enum RomSource {
  InternetArchive(IaSource),
  Folder(FolderSource),
}

/// One disc in a multi-disc game (disc 2 and above).
///
/// Disc 1 stays in the main `RomSourceData`; extra discs live here.
pub struct DiscFile {
  /// Full IA path within the item, or absolute local path for folder sources.
  pub file_name: String,
  /// Basename of the disc file (e.g. `"Game (Disc 2).chd"`).
  pub filename: String,
  /// Download URL — empty for folder sources.
  pub rom_url: String,
  pub sha1: Option<String>,
  #[allow(dead_code)]
  pub md5: Option<String>,
  #[allow(dead_code)]
  pub crc32: Option<String>,
  #[allow(dead_code)]
  pub size: u64,
  /// Absolute path on disk — `Some` for folder sources, `None` for IA.
  pub local_path: Option<PathBuf>,
}

/// Immutable source data for a ROM, set once during collection.
pub struct RomSourceData {
  /// Full path within the IA item, or absolute local path for folder sources.
  /// For multi-disc games this is disc 1's actual path; `filename` is the
  /// virtual logical name shared by the whole group.
  pub file_name: String,
  /// Logical basename used for state key, SS lookup, and output directory.
  /// For single-disc games this equals the actual filename.
  /// For multi-disc games this is the virtual name without the disc indicator
  /// (e.g. `"Panzer Dragoon Saga.chd"`).
  pub filename: String,
  pub source: RomSource,
  /// Extra disc files (disc 2, 3, …).  Empty for single-disc games.
  pub extra_discs: Vec<DiscFile>,
}
