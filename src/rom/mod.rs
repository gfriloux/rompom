mod source;
mod step;

pub use source::{FolderSource, IaSource, RomSource, RomSourceData};
pub use step::{Step, StepData, StepKind, StepStatus};

use std::sync::{Arc, Mutex};

use screenscraper::jeuinfo::JeuInfo;

use crate::{package::Medias, ui::RomBar};

// ── Rom ───────────────────────────────────────────────────────────────────

pub struct Rom {
  pub source: RomSourceData,
  pub pipeline: Vec<Step>,
  pub bar: RomBar,

  // ── Shared inter-step data ─────────────────────────────────────────────
  // Initialised from IaSource at construction (IA) or by ComputeHashes (folder).
  /// SHA-1 of the ROM file.
  pub sha1: Option<String>,
  /// MD5 of the ROM file (only needed for jeuinfo(); absent for fast-path).
  pub md5: Option<String>,
  /// CRC-32 of the ROM file.
  pub crc32: Option<String>,
  /// File size in bytes.
  pub size: u64,
  /// Last-modification timestamp (Unix seconds). 0 for IA sources.
  pub mtime: u64,
  /// True when the ROM sha1 matches the saved state entry → download can be skipped.
  pub rom_unchanged: bool,
  /// Game info from ScreenScraper. Set by LookupSS or WaitModal.
  pub jeu: Option<JeuInfo>,
  /// Media assets resolved by BuildPackage.
  pub medias: Option<Medias>,
  /// Normalised ROM name (set by BuildPackage, used by download workers).
  pub romname: Option<String>,
  /// True when both ROM and all media sha1s match the saved state → build skipped.
  pub package_unchanged: bool,
  /// Per-step decision log lines, appended throughout the pipeline.
  /// Written to `<system>.debug.log` at the end of `SaveState` when `--debug` is set.
  pub debug_log: Vec<String>,
}

impl Rom {
  // ── Folder pipeline ───────────────────────────────────────────────────
  //
  // Index  Kind            wait_for  next
  // ─────  ──────────────  ────────  ────
  //   0    ComputeHashes      0      [1]
  //   1    LookupSS           1      [2]
  //   2    WaitModal          1      [3]     ← Skipped by default
  //   3    BuildPackage       1      [4, 5]
  //   4    CopyRom            1      [6]
  //   5    DownloadMedias     1      [6]
  //   6    SaveState          2      []
  //
  /// Builds a ROM with the folder-source pipeline and returns it wrapped in
  /// `Arc<Mutex<_>>`. The caller must enqueue step 0 into the `TaskQueue`.
  pub fn new_folder(source: RomSourceData, bar: RomBar) -> Arc<Mutex<Self>> {
    let pipeline = vec![
      Step::new(
        StepKind::ComputeHashes,
        StepStatus::Pending,
        StepData::ComputeHashes {
          sha1: None,
          md5: None,
          crc32: None,
          size: 0,
          mtime: 0,
        },
        vec![1],
        0,
      ),
      Step::new(
        StepKind::LookupSS,
        StepStatus::Pending,
        StepData::LookupSS {
          jeu: Box::new(None),
          candidates: Vec::new(),
        },
        vec![2],
        1,
      ),
      Step::new(
        StepKind::WaitModal,
        StepStatus::Skipped,
        StepData::WaitModal {
          jeu: Box::new(None),
        },
        vec![3],
        1,
      ),
      Step::new(
        StepKind::BuildPackage,
        StepStatus::Pending,
        StepData::BuildPackage {
          medias: Box::new(None),
          romname: None,
          pkgver: 0,
        },
        vec![4, 5],
        1,
      ),
      Step::new(
        StepKind::CopyRom,
        StepStatus::Pending,
        StepData::CopyRom,
        vec![6],
        1,
      ),
      Step::new(
        StepKind::DownloadMedias,
        StepStatus::Pending,
        StepData::DownloadMedias,
        vec![6],
        1,
      ),
      Step::new(
        StepKind::SaveState,
        StepStatus::Pending,
        StepData::SaveState,
        vec![],
        2,
      ),
    ];

    Arc::new(Mutex::new(Self {
      source,
      pipeline,
      bar,
      sha1: None,
      md5: None,
      crc32: None,
      size: 0,
      mtime: 0,
      rom_unchanged: false,
      jeu: None,
      medias: None,
      romname: None,
      package_unchanged: false,
      debug_log: Vec::new(),
    }))
  }

  // ── Internet Archive pipeline ─────────────────────────────────────────
  //
  // Index  Kind            wait_for  next
  // ─────  ──────────────  ────────  ────
  //   0    LookupSS           0      [1]
  //   1    WaitModal          1      [2]     ← Skipped by default
  //   2    BuildPackage       1      [3, 4]
  //   3    DownloadRom        1      [5]
  //   4    DownloadMedias     1      [5]
  //   5    SaveState          2      []
  //
  /// Builds a ROM with the Internet Archive pipeline and returns it wrapped in
  /// `Arc<Mutex<_>>`. The caller must enqueue step 0 into the `TaskQueue`.
  pub fn new_ia(source: RomSourceData, bar: RomBar) -> Arc<Mutex<Self>> {
    // Seed shared hash fields from the IA metadata that was already fetched.
    let (sha1, md5, crc32, size) = match &source.source {
      RomSource::InternetArchive(ia) => {
        (ia.sha1.clone(), ia.md5.clone(), ia.crc32.clone(), ia.size)
      }
      _ => unreachable!("new_ia called with non-IA source"),
    };

    let pipeline = vec![
      Step::new(
        StepKind::LookupSS,
        StepStatus::Pending,
        StepData::LookupSS {
          jeu: Box::new(None),
          candidates: Vec::new(),
        },
        vec![1],
        0,
      ),
      Step::new(
        StepKind::WaitModal,
        StepStatus::Skipped,
        StepData::WaitModal {
          jeu: Box::new(None),
        },
        vec![2],
        1,
      ),
      Step::new(
        StepKind::BuildPackage,
        StepStatus::Pending,
        StepData::BuildPackage {
          medias: Box::new(None),
          romname: None,
          pkgver: 0,
        },
        vec![3, 4],
        1,
      ),
      Step::new(
        StepKind::DownloadRom,
        StepStatus::Pending,
        StepData::DownloadRom,
        vec![5],
        1,
      ),
      Step::new(
        StepKind::DownloadMedias,
        StepStatus::Pending,
        StepData::DownloadMedias,
        vec![5],
        1,
      ),
      Step::new(
        StepKind::SaveState,
        StepStatus::Pending,
        StepData::SaveState,
        vec![],
        2,
      ),
    ];

    Arc::new(Mutex::new(Self {
      source,
      pipeline,
      bar,
      sha1,
      md5,
      crc32,
      size,
      mtime: 0,
      rom_unchanged: false,
      jeu: None,
      medias: None,
      romname: None,
      package_unchanged: false,
      debug_log: Vec::new(),
    }))
  }
}
