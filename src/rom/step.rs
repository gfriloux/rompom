use std::{
  sync::atomic::{AtomicUsize, Ordering},
  time::Instant,
};

use crate::ui::ModalCandidate;

// ── Phase ──────────────────────────────────────────────────────────────────

/// Logical pipeline phase for a step.
/// Does not depend on `ui::RomPhase` (which is private to `ui`).
#[allow(dead_code)]
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

// ── StepError ─────────────────────────────────────────────────────────────

/// Why a step handler stopped short.
///
/// The distinction is what `execute_step` retries on. Retrying is not free: each
/// attempt costs a backoff delay, and against ScreenScraper it also costs a request.
/// The API answers 431 — "sort your ROM files out and come back tomorrow" — to members
/// who pile up failed lookups, so replaying a call that cannot possibly succeed makes
/// the run worse, not better.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepError {
  /// Ctrl-C landed while the handler was waiting on a semaphore. The step goes back
  /// to `Pending` and its successors are *not* dispatched, so it re-runs on resume.
  Interrupted,
  /// Worth another go: a timeout, a rate limit, a server having a bad minute.
  Transient(String),
  /// Definitive for this run: exhausted quota, wrong credentials, a bug in rompom.
  /// Fails on the spot, whatever the retry budget says.
  Fatal(String),
}

impl StepError {
  /// Shorthand for `map_err(StepError::transient)`.
  pub fn transient(cause: impl std::fmt::Display) -> Self {
    StepError::Transient(cause.to_string())
  }

  /// Shorthand for `map_err(StepError::fatal)`.
  #[allow(dead_code)]
  pub fn fatal(cause: impl std::fmt::Display) -> Self {
    StepError::Fatal(cause.to_string())
  }
}

impl std::fmt::Display for StepError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      StepError::Interrupted => write!(f, "interrupted"),
      StepError::Transient(m) | StepError::Fatal(m) => write!(f, "{}", m),
    }
  }
}

// ── StepData ──────────────────────────────────────────────────────────────

/// Per-step input/output data.
#[allow(dead_code)]
/// What a step leaves behind for a later one, in the pipeline rather than in the `Rom`.
///
/// There is exactly one such thing. Every other step passes its results through the
/// `Rom` — `sha1`, `jeu`, `medias`, `romname` all live there — and the per-kind variants
/// that used to be here only ever got written: `ComputeHashes` and `BuildPackage` copied
/// values the `Rom` already held, and `WaitModal` stored a `JeuInfo` "for telemetry" that
/// nothing ever read back. Being a second, staler copy of the truth is not a use.
pub enum StepData {
  /// The name-search results, waiting for `WaitModal` to put them in front of the user.
  /// This one is real: `LookupSS` and `WaitModal` run on different pools, so the
  /// candidates cannot simply be handed over.
  LookupSS { candidates: Vec<ModalCandidate> },
  /// Nothing to carry.
  None,
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
