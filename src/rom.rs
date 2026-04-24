use std::{
  path::PathBuf,
  sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
  },
  time::Instant,
};

use internet_archive::metadata::Metadata;
use screenscraper::jeuinfo::JeuInfo;

use crate::{
  package::Medias,
  ui::{ModalCandidate, RomBar},
};

// ── Phase (local enum, used by StepKind::phase()) ─────────────────────────

/// Logical pipeline phase for a step.
/// Used to associate a step with a TUI panel and to drive `RomBar` transitions.
/// Does not depend on `ui::RomPhase` (which is private to `ui.rs`).
#[allow(dead_code)]
pub enum Phase {
  Discovery,
  Packaging,
  Downloads,
  Completed,
}

// ── StepKind ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StepKind {
  ComputeHashes,
  LookupSS,
  /// Waits for the user to identify a ROM via the modal dialog.
  /// Always present in the pipeline; starts as `Skipped` by default.
  /// `LookupSS` sets it to `Pending` when SS returns no match.
  WaitModal,
  BuildPackage,
  CopyRom,
  DownloadRom,
  DownloadMedias,
  SaveState,
}

impl StepKind {
  /// Returns the logical pipeline phase this step belongs to.
  #[allow(dead_code)]
  pub fn phase(&self) -> Phase {
    match self {
      StepKind::ComputeHashes | StepKind::LookupSS | StepKind::WaitModal => Phase::Discovery,
      StepKind::BuildPackage => Phase::Packaging,
      StepKind::CopyRom | StepKind::DownloadRom | StepKind::DownloadMedias => Phase::Downloads,
      StepKind::SaveState => Phase::Completed,
    }
  }

  /// Returns true for steps that must run on `pool_blocking` (i.e. they can
  /// block indefinitely on user input). Only `WaitModal` qualifies.
  pub fn is_blocking(&self) -> bool {
    matches!(self, StepKind::WaitModal)
  }

  /// Maximum number of automatic retries on transient failure.
  /// `0` means no retry — the step fails immediately on error.
  /// Per-media retry logic inside `DownloadMedias` is handled internally.
  pub fn max_retries(&self) -> u8 {
    match self {
      StepKind::LookupSS => 3,
      StepKind::DownloadRom => 5,
      StepKind::DownloadMedias => 3,
      _ => 0,
    }
  }
}

// ── StepStatus ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepStatus {
  /// Waiting for all predecessors to complete.
  Pending,
  /// Currently being executed by a worker.
  InProgress,
  /// Completed successfully.
  Done,
  /// Intentionally bypassed (e.g. `WaitModal` when SS found the game).
  Skipped,
  /// Terminated with an unrecoverable error.
  Failed(String),
}

// ── StepData ──────────────────────────────────────────────────────────────

/// Per-step input/output data.
///
/// Each variant corresponds to a `StepKind`. Output fields start as `None`
/// (or `0` / empty) and are populated by the step handler upon completion.
/// Fields exist for future pipeline inspection and serialisation; not all
/// are consumed within the current pipeline.
#[allow(dead_code)]
pub enum StepData {
  ComputeHashes {
    /// SHA-1 of the local file (filled by handler).
    sha1: Option<String>,
    /// MD5 of the local file (filled by handler).
    md5: Option<String>,
    /// CRC-32 of the local file (filled by handler).
    crc32: Option<String>,
    /// File size in bytes (filled by handler).
    size: u64,
    /// Last-modification timestamp (Unix seconds, filled by handler).
    mtime: u64,
  },
  LookupSS {
    /// Resolved game info from ScreenScraper (filled when found).
    jeu: Box<Option<JeuInfo>>,
    /// Search candidates to display in the modal (filled when not found).
    candidates: Vec<ModalCandidate>,
  },
  WaitModal {
    /// Game info chosen by the user (filled by handler; `None` if cancelled).
    jeu: Box<Option<JeuInfo>>,
  },
  BuildPackage {
    /// Media assets resolved by `Package::new` (filled by handler).
    medias: Box<Option<Medias>>,
    /// Normalized ROM name (filled by handler).
    romname: Option<String>,
    /// PKGBUILD version written (filled by handler).
    pkgver: u32,
  },
  CopyRom,
  DownloadRom,
  DownloadMedias,
  SaveState,
}

// ── Step ──────────────────────────────────────────────────────────────────

pub struct Step {
  pub kind: StepKind,
  pub status: StepStatus,
  pub data: StepData,

  // DAG
  /// Indices of successor steps in `Rom::pipeline`.
  pub next: Vec<usize>,
  /// Number of predecessor steps that must complete before this step can run.
  /// Decremented atomically by the dispatch loop after each predecessor finishes.
  pub wait_for: AtomicUsize,

  // Telemetry
  pub started_at: Option<Instant>,
  pub finished_at: Option<Instant>,

  // Retry
  pub retry_count: u8,
}

impl Step {
  fn new(
    kind: StepKind,
    status: StepStatus,
    data: StepData,
    next: Vec<usize>,
    wait_for: usize,
  ) -> Self {
    Self {
      kind,
      status,
      data,
      next,
      wait_for: AtomicUsize::new(wait_for),
      started_at: None,
      finished_at: None,
      retry_count: 0,
    }
  }

  /// Decrements `wait_for` by 1 and returns the new value.
  pub fn dec_wait_for(&self) -> usize {
    self.wait_for.fetch_sub(1, Ordering::SeqCst) - 1
  }

  /// Returns the current `wait_for` value.
  pub fn wait_for_count(&self) -> usize {
    self.wait_for.load(Ordering::SeqCst)
  }

  /// Returns the configured `max_retries` for this step's kind.
  pub fn max_retries(&self) -> u8 {
    self.kind.max_retries()
  }
}

// ── RomSource ─────────────────────────────────────────────────────────────

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
