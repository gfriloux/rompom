use std::{
  sync::atomic::{AtomicUsize, Ordering},
  time::Instant,
};

use screenscraper::jeuinfo::JeuInfo;

use crate::{package::Medias, ui::ModalCandidate};

// ── Phase ──────────────────────────────────────────────────────────────────

/// Logical pipeline phase for a step.
/// Does not depend on `ui::RomPhase` (which is private to `ui`).
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
#[allow(dead_code)]
pub enum StepData {
  ComputeHashes {
    sha1: Option<String>,
    md5: Option<String>,
    crc32: Option<String>,
    size: u64,
    mtime: u64,
  },
  LookupSS {
    jeu: Box<Option<JeuInfo>>,
    candidates: Vec<ModalCandidate>,
  },
  WaitModal {
    jeu: Box<Option<JeuInfo>>,
  },
  BuildPackage {
    medias: Box<Option<Medias>>,
    romname: Option<String>,
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
  pub wait_for: AtomicUsize,

  // Telemetry
  pub started_at: Option<Instant>,
  pub finished_at: Option<Instant>,

  // Retry
  pub retry_count: u8,
}

impl Step {
  pub fn new(
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
