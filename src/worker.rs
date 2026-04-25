use std::{
  collections::HashMap,
  fs,
  path::Path,
  sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
  },
  time::{Instant, UNIX_EPOCH},
};

use checksums::{hash_file, Algorithm};
use internet_archive::download::{Download, DownloadMethod};
use screenscraper::ScreenScraper;
use serde::{Deserialize, Serialize};

use crate::{
  conf::System,
  package::{read_pkgver, Medias, Package},
  queue::{Semaphore, TaskQueue},
  rom::{Rom, RomSource, StepData, StepKind, StepStatus},
  state::{RomStateEntry, SystemState},
  ui::{ModalCandidate, ModalRequest, ModalResponse},
};

const NAME_REGIONS: &[&str] = &["wor", "eu", "us", "fr", "jp", "ss"];

// ── Run state ─────────────────────────────────────────────────────────────
//
// Serialised to `<system>.run.yml` on Ctrl-C. Loaded on startup to offer
// resumption. Does not contain step *data* (JeuInfo, medias, etc.) — those
// will be re-derived from the existing state.yml and SS API on resume.

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub enum RunStepStatus {
  Pending,
  /// Step was in progress when interrupted; treated as Pending on resume.
  InProgress,
  Done,
  Skipped,
  Failed(String),
}

impl RunStepStatus {
  pub fn is_complete(&self) -> bool {
    matches!(self, Self::Done | Self::Skipped)
  }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RunRomEntry {
  pub filename: String,
  pub step_statuses: Vec<RunStepStatus>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RunState {
  pub roms: Vec<RunRomEntry>,
}

/// Snapshot the current step statuses of all ROMs into a `RunState`.
pub fn collect_run_state(roms: &[Arc<Mutex<Rom>>]) -> RunState {
  RunState {
    roms: roms
      .iter()
      .map(|rom_arc| {
        let rom = rom_arc.lock().unwrap();
        RunRomEntry {
          filename: rom.source.filename.clone(),
          step_statuses: rom
            .pipeline
            .iter()
            .map(|step| match &step.status {
              StepStatus::Pending => RunStepStatus::Pending,
              StepStatus::InProgress => RunStepStatus::InProgress,
              StepStatus::Done => RunStepStatus::Done,
              StepStatus::Skipped => RunStepStatus::Skipped,
              StepStatus::Failed(e) => RunStepStatus::Failed(e.clone()),
            })
            .collect(),
        }
      })
      .collect(),
  }
}

/// Write a `RunState` to `<system_name>.run.yml`.
pub fn save_run_state(system_name: &str, state: &RunState) -> std::io::Result<()> {
  let path = format!("{}.run.yml", system_name);
  let yaml = serde_yaml::to_string(state).map_err(std::io::Error::other)?;
  std::fs::write(path, yaml)
}

/// Load a `RunState` from disk.
pub fn load_run_state(path: &str) -> Result<RunState, Box<dyn std::error::Error>> {
  let content = std::fs::read_to_string(path)?;
  let state = serde_yaml::from_str(&content)?;
  Ok(state)
}

/// Apply a previously saved run state to an already-constructed ROM.
///
/// Steps that were Done or Skipped are restored and their successors'
/// `wait_for` counters are decremented accordingly, so the next call to
/// "enqueue ready steps" will correctly identify which steps still need
/// to run.
///
/// Steps that were InProgress are treated as Pending (re-run from scratch).
pub fn apply_run_state(rom: &mut Rom, run_entry: &RunRomEntry) {
  for (idx, saved) in run_entry.step_statuses.iter().enumerate() {
    if idx >= rom.pipeline.len() {
      break;
    }
    let restored = match saved {
      RunStepStatus::Done => StepStatus::Done,
      RunStepStatus::Skipped => StepStatus::Skipped,
      RunStepStatus::Failed(e) => StepStatus::Failed(e.clone()),
      // Pending or InProgress → stay Pending (initial DAG status)
      _ => continue,
    };
    rom.pipeline[idx].status = restored;

    // Completed step: decrement each successor's wait_for.
    let nexts: Vec<usize> = rom.pipeline[idx].next.clone();
    for next_idx in nexts {
      rom.pipeline[next_idx].dec_wait_for();
    }
  }
}

// ── Worker context ─────────────────────────────────────────────────────────

pub struct WorkerContext {
  pub queue: Arc<TaskQueue>,
  pub ss: Arc<ScreenScraper>,
  pub system: Arc<System>,
  pub lang: Arc<Vec<String>>,
  pub state: Arc<Mutex<SystemState>>,
  pub modal_tx: crossbeam_channel::Sender<ModalRequest>,
  pub ss_sem: Arc<Semaphore>,
  pub modal_sem: Arc<Semaphore>,
  /// Number of ROMs whose `SaveState` step has not yet completed.
  /// When it reaches zero the queue is shut down.
  pub remaining: Arc<AtomicUsize>,
  /// Set to true by the Ctrl-C handler; workers check it between steps.
  pub interrupted: Arc<AtomicBool>,
  /// If `Some`, path of the debug log file to append per-ROM decision lines to.
  /// Enabled by `--debug`; the file is created/truncated in `main` before workers start.
  pub debug_log_path: Option<String>,
}

// ── Worker loops ───────────────────────────────────────────────────────────

/// Main worker — handles every step except `WaitModal`.
pub fn worker_loop_main(ctx: Arc<WorkerContext>) {
  while let Some((rom_arc, step_idx)) = ctx.queue.pop_main() {
    if ctx.interrupted.load(Ordering::Relaxed) {
      break;
    }
    execute_step(rom_arc, step_idx, &ctx);
  }
}

/// Blocking worker — handles `WaitModal` steps that need user input.
pub fn worker_loop_blocking(ctx: Arc<WorkerContext>) {
  while let Some((rom_arc, step_idx)) = ctx.queue.pop_blocking() {
    if ctx.interrupted.load(Ordering::Relaxed) {
      break;
    }
    execute_step(rom_arc, step_idx, &ctx);
  }
}

// ── Step execution ─────────────────────────────────────────────────────────

fn execute_step(rom_arc: Arc<Mutex<Rom>>, step_idx: usize, ctx: &WorkerContext) {
  // Fast path: Skipped steps are no-ops — dispatch successors and return.
  {
    let rom = rom_arc.lock().unwrap();
    if rom.pipeline[step_idx].status == StepStatus::Skipped {
      drop(rom);
      do_dispatch(&rom_arc, step_idx, &ctx.queue);
      return;
    }
  }

  // Mark step as InProgress.
  {
    let mut rom = rom_arc.lock().unwrap();
    let step = &mut rom.pipeline[step_idx];
    step.status = StepStatus::InProgress;
    step.started_at = Some(Instant::now());
  }

  // Dispatch to per-step handler.
  let kind: StepKind = {
    let rom = rom_arc.lock().unwrap();
    rom.pipeline[step_idx].kind.clone()
  };

  let result = match kind {
    StepKind::ComputeHashes => handle_compute_hashes(&rom_arc, step_idx, ctx),
    StepKind::LookupSS => handle_lookup_ss(&rom_arc, step_idx, ctx),
    StepKind::WaitModal => handle_wait_modal(&rom_arc, step_idx, ctx),
    StepKind::BuildPackage => handle_build_package(&rom_arc, step_idx, ctx),
    StepKind::CopyRom => handle_copy_rom(&rom_arc, step_idx, ctx),
    StepKind::DownloadRom => handle_download_rom(&rom_arc, step_idx, ctx),
    StepKind::DownloadMedias => handle_download_medias(&rom_arc, step_idx, ctx),
    StepKind::SaveState => handle_save_state(&rom_arc, step_idx, ctx),
  };

  // Resolve final step status, handling retries.
  let final_status = match result {
    Ok(s) => s,
    Err(msg) => {
      let (retry_count, max_retries) = {
        let rom = rom_arc.lock().unwrap();
        let step = &rom.pipeline[step_idx];
        (step.retry_count, step.max_retries())
      };
      if retry_count < max_retries {
        // Increment retry counter and re-enqueue with exponential backoff.
        {
          let mut rom = rom_arc.lock().unwrap();
          rom.pipeline[step_idx].retry_count += 1;
          rom.pipeline[step_idx].status = StepStatus::Pending;
        }
        // Sleep 2^retry_count seconds (1s, 2s, 4s, 8s, …) before retrying.
        let delay = std::time::Duration::from_secs(1u64 << retry_count);
        std::thread::sleep(delay);
        ctx.queue.push(Arc::clone(&rom_arc), step_idx);
        return;
      }
      // Exhausted retries: mark as failed and notify the UI.
      {
        let rom = rom_arc.lock().unwrap();
        rom.bar.finish_error();
      }
      StepStatus::Failed(msg)
    }
  };

  // Record completion timestamp and final status.
  {
    let mut rom = rom_arc.lock().unwrap();
    let step = &mut rom.pipeline[step_idx];
    step.finished_at = Some(Instant::now());
    step.status = final_status;
  }

  // DAG routing: decrement successors' wait_for and enqueue those that are ready.
  do_dispatch(&rom_arc, step_idx, &ctx.queue);
}

/// Decrement `wait_for` for each successor of `step_idx`.
/// Enqueues any successor whose counter reaches zero.
fn do_dispatch(rom_arc: &Arc<Mutex<Rom>>, step_idx: usize, queue: &Arc<TaskQueue>) {
  let nexts: Vec<usize> = {
    let rom = rom_arc.lock().unwrap();
    rom.pipeline[step_idx].next.clone()
  };
  for next_idx in nexts {
    let remaining = {
      let rom = rom_arc.lock().unwrap();
      rom.pipeline[next_idx].dec_wait_for()
    };
    if remaining == 0 {
      queue.push(Arc::clone(rom_arc), next_idx);
    }
  }
}

// ── Shared helpers ─────────────────────────────────────────────────────────

/// Strips the file extension and region/revision tags from a ROM filename to
/// produce a clean title suitable for a ScreenScraper name search.
///
/// `"Sonic The Hedgehog (USA) [!].zip"` → `"Sonic The Hedgehog"`
pub(crate) fn search_name(filename: &str) -> String {
  let stem = Path::new(filename)
    .file_stem()
    .and_then(|s| s.to_str())
    .unwrap_or(filename);
  stem
    .split('(')
    .next()
    .and_then(|s| s.split('[').next())
    .unwrap_or(stem)
    .trim()
    .to_string()
}

/// Returns the output filename for a downloaded media asset.
fn media_filename(kind: &str, format: &str) -> String {
  match kind {
    "video" => "video.mp4".to_string(),
    "manual" => "manual.pdf".to_string(),
    _ => format!("{}.{}", kind, format),
  }
}

/// Compares current media sha1s (from SS) against the saved state.
///
/// Returns `(changed, log_lines)` where `changed` is true if at least one
/// media sha1 differs, and `log_lines` has one entry per media type with
/// the comparison result (for `--debug` output).
fn check_media_changes(
  medias: &Medias,
  prev: &HashMap<String, Option<String>>,
) -> (bool, Vec<String>) {
  let mut changed = false;
  let mut lines = Vec::new();

  for (kind, media) in [
    ("video", medias.video.as_ref()),
    ("image", medias.image.as_ref()),
    ("thumbnail", medias.thumbnail.as_ref()),
    ("bezel", medias.bezel.as_ref()),
    ("marquee", medias.marquee.as_ref()),
    ("screenshot", medias.screenshot.as_ref()),
    ("wheel", medias.wheel.as_ref()),
    ("manual", medias.manual.as_ref()),
  ] {
    let new_sha1 = media.map(|m| m.sha1.as_str());
    let prev_sha1 = prev.get(kind).and_then(|v| v.as_deref());
    if new_sha1 != prev_sha1 {
      changed = true;
      lines.push(format!(
        "[BuildPackage] media {:<12}: CHANGED  state={}  ss={}",
        kind,
        prev_sha1.unwrap_or("(absent)"),
        new_sha1.unwrap_or("(absent)")
      ));
    } else {
      lines.push(format!(
        "[BuildPackage] media {:<12}: ok       ({})",
        kind,
        new_sha1.unwrap_or("absent")
      ));
    }
  }

  (changed, lines)
}

// ── Discovery handlers ─────────────────────────────────────────────────────

/// Compute SHA-1/MD5/CRC-32 for a folder-source ROM.
///
/// Fast-skip: if the saved state has a matching mtime + size for this file,
/// restore sha1 from state and skip the (expensive) full hash computation.
fn handle_compute_hashes(
  rom_arc: &Arc<Mutex<Rom>>,
  _step_idx: usize,
  ctx: &WorkerContext,
) -> Result<StepStatus, String> {
  let (filename, local_path) = {
    let rom = rom_arc.lock().unwrap();
    let path = match &rom.source.source {
      RomSource::Folder(f) => f.local_path.clone(),
      _ => unreachable!("ComputeHashes only runs on folder sources"),
    };
    (rom.source.filename.clone(), path)
  };

  rom_arc.lock().unwrap().bar.discovering();

  // ── Fast-skip: check mtime + size against saved state ─────────────────
  let fast_result: Option<(String, u64, u64)> = {
    let state = ctx.state.lock().unwrap();
    state.roms.get(&filename).and_then(|entry| {
      if entry.rom_mtime == 0 {
        return None; // no mtime recorded yet
      }
      let meta = fs::metadata(&local_path).ok()?;
      let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())?;
      if mtime == entry.rom_mtime && meta.len() == entry.rom_size {
        Some((entry.rom_sha1.clone(), mtime, meta.len()))
      } else {
        None
      }
    })
  };

  if let Some((sha1, mtime, size)) = fast_result {
    let mut rom = rom_arc.lock().unwrap();
    rom.sha1 = Some(sha1.clone());
    rom.mtime = mtime;
    rom.size = size;
    // md5/crc32 stay None — jeuinfo_by_gameid won't need them (cached game_id)
    rom.debug_log.push(format!(
      "[ComputeHashes] fast-path: HIT  (mtime={}, size={}, sha1={})",
      mtime, size, sha1
    ));
  } else {
    let sha1 = hash_file(&local_path, Algorithm::SHA1).to_lowercase();
    let md5 = hash_file(&local_path, Algorithm::MD5).to_lowercase();
    let crc32 = hash_file(&local_path, Algorithm::CRC32).to_lowercase();
    let meta = fs::metadata(&local_path).ok();
    let mtime = meta
      .as_ref()
      .and_then(|m| m.modified().ok())
      .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
      .map(|d| d.as_secs())
      .unwrap_or(0);
    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);

    let mut rom = rom_arc.lock().unwrap();
    rom.debug_log.push(format!(
      "[ComputeHashes] fast-path: MISS → computed sha1={} (mtime={}, size={})",
      sha1, mtime, size
    ));
    rom.sha1 = Some(sha1);
    rom.md5 = Some(md5);
    rom.crc32 = Some(crc32);
    rom.mtime = mtime;
    rom.size = size;
  }

  // ── Check if ROM is unchanged based on saved state ────────────────────
  let sha1_now: Option<String> = rom_arc.lock().unwrap().sha1.clone();
  let (unchanged, state_rom_sha1) = {
    let state = ctx.state.lock().unwrap();
    match state.roms.get(&filename) {
      None => (None, None),
      Some(entry) => {
        let current = sha1_now.as_deref().unwrap_or("");
        let u = !current.is_empty() && entry.rom_sha1 == current;
        (Some(u), Some(entry.rom_sha1.clone()))
      }
    }
  };
  {
    let mut rom = rom_arc.lock().unwrap();
    rom.rom_unchanged = unchanged.unwrap_or(false);
    let current = sha1_now.as_deref().unwrap_or("?");
    let line = match (unchanged, state_rom_sha1.as_deref()) {
      (None, _) => format!(
        "[ComputeHashes] rom_unchanged: false — no state entry (current sha1={})",
        current
      ),
      (Some(true), _) => format!("[ComputeHashes] rom_unchanged: true  (sha1={})", current),
      (Some(false), Some(s)) => format!(
        "[ComputeHashes] rom_unchanged: false — sha1 mismatch (state={}, current={})",
        s, current
      ),
      (Some(false), None) => format!(
        "[ComputeHashes] rom_unchanged: false — state sha1 empty (current={})",
        current
      ),
    };
    rom.debug_log.push(line);
  }

  Ok(StepStatus::Done)
}

/// Look up the ROM in ScreenScraper via sha1/crc32/md5 or a cached game ID.
///
/// - Found → stores `JeuInfo` in `rom.jeu`, calls `bar.found()`, transitions
///   bar to Packaging/waiting.
/// - Not found → calls `jeu_recherche` to populate modal candidates, sets
///   `WaitModal` status to `Pending` so the blocking worker handles it.
fn handle_lookup_ss(
  rom_arc: &Arc<Mutex<Rom>>,
  step_idx: usize,
  ctx: &WorkerContext,
) -> Result<StepStatus, String> {
  // ── Read source data from rom (release lock before network calls) ──────
  let (filename, sha1, md5, crc32, size, is_ia_source) = {
    let rom = rom_arc.lock().unwrap();
    let is_ia = matches!(rom.source.source, RomSource::InternetArchive(_));
    (
      rom.source.filename.clone(),
      rom.sha1.clone(),
      rom.md5.clone(),
      rom.crc32.clone(),
      rom.size,
      is_ia,
    )
  };

  rom_arc.lock().unwrap().bar.discovering();

  // ── For IA sources: determine rom_unchanged here (no ComputeHashes ran) ─
  if is_ia_source {
    let (unchanged, state_rom_sha1) = {
      let state = ctx.state.lock().unwrap();
      match state.roms.get(&filename) {
        None => (None, None),
        Some(entry) => {
          let current = sha1.as_deref().unwrap_or("");
          let u = !current.is_empty() && entry.rom_sha1 == current;
          (Some(u), Some(entry.rom_sha1.clone()))
        }
      }
    };
    let mut rom = rom_arc.lock().unwrap();
    rom.rom_unchanged = unchanged.unwrap_or(false);
    let current = sha1.as_deref().unwrap_or("?");
    let line = match (unchanged, state_rom_sha1.as_deref()) {
      (None, _) => format!(
        "[LookupSS] rom_unchanged: false — no state entry (current sha1={})",
        current
      ),
      (Some(true), _) => format!("[LookupSS] rom_unchanged: true  (sha1={})", current),
      (Some(false), Some(s)) => format!(
        "[LookupSS] rom_unchanged: false — sha1 mismatch (state={}, current={})",
        s, current
      ),
      (Some(false), None) => format!(
        "[LookupSS] rom_unchanged: false — state sha1 empty (current={})",
        current
      ),
    };
    rom.debug_log.push(line);
  }

  // ── Check state for a cached SS game ID ───────────────────────────────
  let cached_game_id: Option<u32> = {
    let state = ctx.state.lock().unwrap();
    state
      .roms
      .get(&filename)
      .and_then(|e| e.ss_game_id.as_deref())
      .and_then(|id| id.parse().ok())
  };

  // ── SS lookup (semaphore limits concurrency to user's SS tier) ────────
  ctx.ss_sem.acquire();
  let ji = if let Some(gid) = cached_game_id {
    ctx.ss.jeuinfo_by_gameid(ctx.system.id, gid).ok()
  } else {
    ctx
      .ss
      .jeuinfo(ctx.system.id, &filename, size, crc32, md5, sha1)
      .ok()
  };
  ctx.ss_sem.release();

  if let Some(jeu) = ji {
    // ── Found ─────────────────────────────────────────────────────────
    let name = jeu.find_name(NAME_REGIONS);
    {
      let mut rom = rom_arc.lock().unwrap();
      rom.jeu = Some(jeu);
      rom.bar.found(&name);
    }
    // Transition bar to Packaging/waiting (WaitModal will be Skipped).
    rom_arc.lock().unwrap().bar.preparing_pending();
    Ok(StepStatus::Done)
  } else {
    // ── Not found: run jeu_recherche and hand off to WaitModal ────────
    ctx.ss_sem.acquire();
    let search_results = ctx
      .ss
      .jeu_recherche(Some(ctx.system.id), &search_name(&filename))
      .unwrap_or_default();
    ctx.ss_sem.release();

    let display_candidates: Vec<ModalCandidate> = search_results
      .iter()
      .map(|j| {
        let date = j.find_date(&["wor", "eu", "us", "fr"]);
        ModalCandidate {
          name: j.find_name(NAME_REGIONS),
          game_id: j.id.clone(),
          year: if date == "Unknown" || date.len() < 4 {
            None
          } else {
            Some(date[..4].to_string())
          },
        }
      })
      .collect();

    // Store candidates in this step's data and unlock WaitModal.
    {
      let mut rom = rom_arc.lock().unwrap();
      if let StepData::LookupSS {
        ref mut candidates, ..
      } = rom.pipeline[step_idx].data
      {
        *candidates = display_candidates;
      }
      // WaitModal is always the next step after LookupSS.
      let wait_idx = rom
        .pipeline
        .iter()
        .position(|s| s.kind == StepKind::WaitModal)
        .expect("WaitModal step not found in pipeline");
      rom.pipeline[wait_idx].status = StepStatus::Pending;
    }

    Ok(StepStatus::Done)
  }
}

/// Block until the user identifies the ROM via the modal dialog.
///
/// Acquires `modal_sem` (capacity 1) to serialise modals, sends a
/// `ModalRequest`, and blocks on the response channel.  After the user
/// responds (or cancels), stores the resolved `JeuInfo` in `rom.jeu` and
/// transitions the bar to Packaging/waiting.
fn handle_wait_modal(
  rom_arc: &Arc<Mutex<Rom>>,
  step_idx: usize,
  ctx: &WorkerContext,
) -> Result<StepStatus, String> {
  // Read the candidates that LookupSS stored in its step data.
  let (filename, sha1_opt, candidates) = {
    let rom = rom_arc.lock().unwrap();

    // LookupSS is always the step immediately before WaitModal.
    let lookup_idx = rom
      .pipeline
      .iter()
      .position(|s| s.kind == StepKind::LookupSS)
      .expect("LookupSS step not found in pipeline");

    let candidates = match &rom.pipeline[lookup_idx].data {
      StepData::LookupSS { candidates, .. } => candidates.clone(),
      _ => Vec::new(),
    };

    (rom.source.filename.clone(), rom.sha1.clone(), candidates)
  };

  // Signal the UI that we're waiting for user input.
  rom_arc.lock().unwrap().bar.waiting_for_user();

  // Serialise modal display: only one modal open at a time.
  ctx.modal_sem.acquire();

  let (resp_tx, resp_rx) = crossbeam_channel::bounded::<ModalResponse>(1);
  let ss_for_closure = Arc::clone(&ctx.ss);
  let system_id = ctx.system.id;

  ctx
    .modal_tx
    .send(ModalRequest {
      filename: filename.clone(),
      sha1: sha1_opt,
      candidates,
      response: resp_tx,
      // Called by the TUI render thread to show a confirmation after manual ID entry.
      fetch_by_id: Box::new(move |game_id| {
        ss_for_closure
          .jeuinfo_by_gameid(system_id, game_id)
          .ok()
          .map(|j| j.find_name(NAME_REGIONS).to_string())
      }),
    })
    .map_err(|e| format!("modal channel closed: {}", e))?;

  let response = resp_rx
    .recv()
    .map_err(|_| "modal response channel closed".to_string())?;

  ctx.modal_sem.release();

  // ── Resolve JeuInfo from the user's response ───────────────────────────
  let jeu = match response {
    ModalResponse::SelectedId(id) | ModalResponse::ManualId(id) => {
      id.parse::<u32>().ok().and_then(|gid| {
        ctx.ss_sem.acquire();
        let result = ctx.ss.jeuinfo_by_gameid(ctx.system.id, gid).ok();
        ctx.ss_sem.release();
        result
      })
    }
    ModalResponse::Cancelled => None,
  };

  // ── Update rom and bar ─────────────────────────────────────────────────
  if let Some(ref j) = jeu {
    let name = j.find_name(NAME_REGIONS);
    let mut rom = rom_arc.lock().unwrap();
    rom.bar.found(&name);
    rom.jeu = jeu;
  } else {
    rom_arc.lock().unwrap().bar.not_found();
  }

  // Transition bar to Packaging/waiting regardless of found/cancelled.
  rom_arc.lock().unwrap().bar.preparing_pending();

  // Store jeu also in the WaitModal step data (optional, for telemetry).
  {
    let mut rom = rom_arc.lock().unwrap();
    let jeu_clone = rom.jeu.clone();
    if let StepData::WaitModal { ref mut jeu, .. } = rom.pipeline[step_idx].data {
      **jeu = jeu_clone;
    }
  }

  Ok(StepStatus::Done)
}

// ── Packaging handler ──────────────────────────────────────────────────────

/// Build the PKGBUILD and description.xml for a ROM.
///
/// Skips the build if neither the ROM nor any media sha1 has changed since
/// the last run (`package_unchanged = true`).
fn handle_build_package(
  rom_arc: &Arc<Mutex<Rom>>,
  _step_idx: usize,
  ctx: &WorkerContext,
) -> Result<StepStatus, String> {
  // Extract what we need, releasing the lock before expensive I/O.
  let (filename, sha1, rom_url, jeu, rom_unchanged) = {
    let mut rom = rom_arc.lock().unwrap();
    let jeu = rom.jeu.take(); // Package::new takes ownership; we'll put it back
    let sha1 = rom.sha1.clone().unwrap_or_default();
    let rom_url = match &rom.source.source {
      RomSource::InternetArchive(ia) => ia.rom_url.clone(),
      RomSource::Folder(_) => String::new(),
    };
    (
      rom.source.filename.clone(),
      sha1,
      rom_url,
      jeu,
      rom.rom_unchanged,
    )
  };

  rom_arc.lock().unwrap().bar.preparing();

  let mut package = Package::new(jeu, &filename, &rom_url, &sha1).map_err(|e| e.to_string())?;

  let lang_refs: Vec<&str> = ctx.lang.iter().map(|s| s.as_str()).collect();

  // Check if description.xml content would change (pure read, no I/O side effect).
  let description_changed = package.check_description_changed(&ctx.system, &lang_refs);

  // ── Delta check: skip build if ROM + all media sha1s + description are unchanged ─
  let (package_changed, debug_lines) = {
    let state = ctx.state.lock().unwrap();
    match state.roms.get(&filename) {
      None => (
        true,
        vec!["[BuildPackage] no state entry → package_changed: true".to_string()],
      ),
      Some(prev) => {
        if !rom_unchanged {
          let line = format!(
            "[BuildPackage] rom_unchanged: false → package_changed: true\n  rom sha1: state={}, current={}",
            prev.rom_sha1, sha1
          );
          (true, vec![line])
        } else {
          let (media_changed, mut lines) = check_media_changes(&package.medias, &prev.medias);
          if description_changed {
            lines.push("[BuildPackage] description.xml  : CHANGED".to_string());
          } else {
            lines.push("[BuildPackage] description.xml  : ok       (unchanged)".to_string());
          }
          if media_changed || description_changed {
            lines.push(
              "[BuildPackage] → package_changed: true (media or description mismatch above)"
                .to_string(),
            );
            (true, lines)
          } else {
            lines.push("[BuildPackage] → package_unchanged: true".to_string());
            (false, lines)
          }
        }
      }
    }
  };
  rom_arc.lock().unwrap().debug_log.extend(debug_lines);

  if package_changed {
    let dir = Path::new(&filename).with_extension("");
    let pkgver = read_pkgver(&dir) + 1;
    package
      .build(&ctx.system, &lang_refs, pkgver)
      .map_err(|e| e.to_string())?;
  }

  // Show description.xml icon: green if written/updated, gray if unchanged.
  if description_changed {
    rom_arc.lock().unwrap().bar.media_done("description");
  } else {
    rom_arc.lock().unwrap().bar.media_skipped("description");
  }

  // Move results back into rom.
  let romname = package.normalize_name();
  let medias = package.medias;
  {
    let mut rom = rom_arc.lock().unwrap();
    rom.jeu = package.jeu;
    rom.medias = Some(medias);
    rom.romname = Some(romname);
    rom.package_unchanged = !package_changed;
  }

  rom_arc.lock().unwrap().bar.downloading_pending();
  Ok(StepStatus::Done)
}

// ── Download handlers ──────────────────────────────────────────────────────

/// Copy a local folder-source ROM to its output directory.
fn handle_copy_rom(
  rom_arc: &Arc<Mutex<Rom>>,
  _step_idx: usize,
  _ctx: &WorkerContext,
) -> Result<StepStatus, String> {
  let (filename, sha1_expected, local_path, rom_unchanged) = {
    let rom = rom_arc.lock().unwrap();
    let local_path = match &rom.source.source {
      RomSource::Folder(f) => f.local_path.clone(),
      _ => unreachable!("CopyRom only runs on folder sources"),
    };
    (
      rom.source.filename.clone(),
      rom.sha1.clone().unwrap_or_default(),
      local_path,
      rom.rom_unchanged,
    )
  };

  let directory = Path::new(&filename).with_extension("");
  let dest = directory.join(&filename);

  if rom_unchanged {
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    rom_arc.lock().unwrap().bar.rom_skipped();
    return Ok(StepStatus::Done);
  }

  if dest.exists() {
    rom_arc.lock().unwrap().bar.rom_checking();
    let actual = hash_file(&dest, Algorithm::SHA1).to_lowercase();
    if actual == sha1_expected {
      rom_arc.lock().unwrap().bar.rom_skipped();
    } else {
      rom_arc.lock().unwrap().bar.rom_redownloading();
      fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
      fs::copy(&local_path, &dest).map_err(|e| e.to_string())?;
      rom_arc.lock().unwrap().bar.rom_done();
    }
  } else {
    rom_arc.lock().unwrap().bar.rom_downloading();
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    fs::copy(&local_path, &dest).map_err(|e| e.to_string())?;
    rom_arc.lock().unwrap().bar.rom_done();
  }

  Ok(StepStatus::Done)
}

/// Download a ROM from Internet Archive to its output directory.
fn handle_download_rom(
  rom_arc: &Arc<Mutex<Rom>>,
  _step_idx: usize,
  _ctx: &WorkerContext,
) -> Result<StepStatus, String> {
  let (filename, file_name_in_item, metadata, rom_unchanged) = {
    let rom = rom_arc.lock().unwrap();
    let (metadata, file_name) = match &rom.source.source {
      RomSource::InternetArchive(ia) => (Arc::clone(&ia.metadata), rom.source.file_name.clone()),
      _ => unreachable!("DownloadRom only runs on IA sources"),
    };
    (
      rom.source.filename.clone(),
      file_name,
      metadata,
      rom.rom_unchanged,
    )
  };

  let directory = Path::new(&filename).with_extension("");
  let dest = directory.join(&filename);
  let download = Download::new(&metadata, &file_name_in_item).map_err(|e| e.to_string())?;

  if rom_unchanged {
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    rom_arc.lock().unwrap().bar.rom_skipped();
    return Ok(StepStatus::Done);
  }

  if dest.exists() {
    rom_arc.lock().unwrap().bar.rom_checking();
    match download.verify_sha1(&dest) {
      Ok(()) => {
        rom_arc.lock().unwrap().bar.rom_skipped();
      }
      Err(_) => {
        rom_arc.lock().unwrap().bar.rom_redownloading();
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        download
          .fetch(&dest, DownloadMethod::Https)
          .map_err(|e| e.to_string())?;
        download.verify_sha1(&dest).map_err(|e| e.to_string())?;
        rom_arc.lock().unwrap().bar.rom_done();
      }
    }
  } else {
    rom_arc.lock().unwrap().bar.rom_downloading();
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    download
      .fetch(&dest, DownloadMethod::Https)
      .map_err(|e| e.to_string())?;
    download.verify_sha1(&dest).map_err(|e| e.to_string())?;
    rom_arc.lock().unwrap().bar.rom_done();
  }

  Ok(StepStatus::Done)
}

/// Download all available media assets for a ROM.
///
/// Iterates over the 8 canonical media types in order. Already-valid files
/// are skipped (sha1 verified). Updates the bar icons for each type.
///
/// Takes `rom.medias` out temporarily to avoid holding the Rom lock during
/// downloads, then puts it back on completion.
fn handle_download_medias(
  rom_arc: &Arc<Mutex<Rom>>,
  _step_idx: usize,
  ctx: &WorkerContext,
) -> Result<StepStatus, String> {
  let (filename, medias) = {
    let mut rom = rom_arc.lock().unwrap();
    let filename = rom.source.filename.clone();
    let medias = rom.medias.take(); // temporarily take ownership
    (filename, medias)
  };

  let directory = Path::new(&filename).with_extension("");

  if let Some(ref medias) = medias {
    for (kind, maybe_media) in [
      ("video", medias.video.as_ref()),
      ("image", medias.image.as_ref()),
      ("thumbnail", medias.thumbnail.as_ref()),
      ("bezel", medias.bezel.as_ref()),
      ("marquee", medias.marquee.as_ref()),
      ("screenshot", medias.screenshot.as_ref()),
      ("wheel", medias.wheel.as_ref()),
      ("manual", medias.manual.as_ref()),
    ] {
      match maybe_media {
        Some(m) => {
          rom_arc.lock().unwrap().bar.start_media(kind);
          let dest = directory.join(media_filename(kind, &m.format));
          let needs_download =
            !dest.exists() || ctx.ss.media_download(m).verify_sha1(&dest).is_err();
          if needs_download {
            ctx
              .ss
              .media_download(m)
              .fetch(&dest)
              .map_err(|e| format!("media {}: {}", kind, e))?;
            rom_arc.lock().unwrap().bar.media_done(kind);
          } else {
            rom_arc.lock().unwrap().bar.media_skipped(kind);
          }
        }
        None => {
          rom_arc.lock().unwrap().bar.media_unavailable(kind);
        }
      }
    }
  }

  // Restore medias so SaveState can record their sha1s.
  rom_arc.lock().unwrap().medias = medias;

  Ok(StepStatus::Done)
}

// ── SaveState handler ──────────────────────────────────────────────────────

/// Build a `HashMap<kind, Option<sha1>>` from a `Medias` struct.
fn medias_to_sha1_map(medias: &Medias) -> HashMap<String, Option<String>> {
  let mut map = HashMap::new();
  for (kind, media) in [
    ("video", medias.video.as_ref()),
    ("image", medias.image.as_ref()),
    ("thumbnail", medias.thumbnail.as_ref()),
    ("bezel", medias.bezel.as_ref()),
    ("marquee", medias.marquee.as_ref()),
    ("screenshot", medias.screenshot.as_ref()),
    ("wheel", medias.wheel.as_ref()),
    ("manual", medias.manual.as_ref()),
  ] {
    map.insert(kind.to_string(), media.map(|m| m.sha1.clone()));
  }
  map
}

/// Record this ROM's `RomStateEntry` into the shared in-memory `SystemState`,
/// signal the UI, and shut down the queue once all ROMs are done.
///
/// The state file is flushed to disk once by `main.rs` after all workers join.
fn handle_save_state(
  rom_arc: &Arc<Mutex<Rom>>,
  _step_idx: usize,
  ctx: &WorkerContext,
) -> Result<StepStatus, String> {
  // Collect ROM data while holding the lock, then release before I/O.
  let (filename, entry, package_unchanged, debug_log) = {
    let rom = rom_arc.lock().unwrap();
    let ss_game_id = rom.jeu.as_ref().map(|j| j.id.clone());
    let medias = rom
      .medias
      .as_ref()
      .map(medias_to_sha1_map)
      .unwrap_or_default();
    let state_entry = RomStateEntry {
      ss_game_id,
      rom_sha1: rom.sha1.clone().unwrap_or_default(),
      rom_mtime: rom.mtime,
      rom_size: rom.size,
      medias,
    };
    (
      rom.source.filename.clone(),
      state_entry,
      rom.package_unchanged,
      rom.debug_log.clone(),
    )
  };

  // Persist in memory — main.rs flushes to disk after all workers finish.
  ctx.state.lock().unwrap().insert(filename.clone(), entry);

  // ── Debug log ─────────────────────────────────────────────────────────────
  if let Some(ref path) = ctx.debug_log_path {
    if !debug_log.is_empty() {
      use std::io::Write as _;
      match fs::OpenOptions::new().create(true).append(true).open(path) {
        Ok(mut file) => {
          let _ = writeln!(file, "=== {} ===", filename);
          for line in &debug_log {
            let _ = writeln!(file, "{}", line);
          }
          let _ = writeln!(file);
        }
        Err(e) => eprintln!("Warning: could not write debug log: {}", e),
      }
    }
  }

  {
    let rom = rom_arc.lock().unwrap();
    rom.bar.finish(package_unchanged);
  }

  if ctx.remaining.fetch_sub(1, Ordering::SeqCst) == 1 {
    ctx.queue.shutdown();
  }

  Ok(StepStatus::Done)
}
