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

/// Immutable source data for a ROM, set once during collection.
pub struct RomSourceData {
  /// Full path within the IA item, or absolute local path for folder sources.
  pub file_name: String,
  /// Basename only, used for SS lookup and output paths.
  pub filename: String,
  pub source: RomSource,
}
