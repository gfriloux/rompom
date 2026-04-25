use std::{
  fs,
  sync::{Arc, Mutex},
  time::UNIX_EPOCH,
};

use checksums::{hash_file, Algorithm};

use crate::{
  rom::{Rom, RomSource, StepData, StepKind, StepStatus},
  ui::{ModalCandidate, ModalRequest, ModalResponse},
};

use super::super::{
  helpers::{search_name, NAME_REGIONS},
  WorkerContext,
};

// ── ComputeHashes ─────────────────────────────────────────────────────────

/// Compute SHA-1/MD5/CRC-32 for a folder-source ROM.
///
/// Fast-skip: if the saved state has a matching mtime + size for this file,
/// restore sha1 from state and skip the (expensive) full hash computation.
pub(crate) fn handle_compute_hashes(
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

// ── LookupSS ──────────────────────────────────────────────────────────────

/// Look up the ROM in ScreenScraper via sha1/crc32/md5 or a cached game ID.
///
/// - Found → stores `JeuInfo` in `rom.jeu`, calls `bar.found()`, transitions
///   bar to Packaging/waiting.
/// - Not found → calls `jeu_recherche` to populate modal candidates, sets
///   `WaitModal` status to `Pending` so the blocking worker handles it.
pub(crate) fn handle_lookup_ss(
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

// ── WaitModal ─────────────────────────────────────────────────────────────

/// Block until the user identifies the ROM via the modal dialog.
///
/// Acquires `modal_sem` (capacity 1) to serialise modals, sends a
/// `ModalRequest`, and blocks on the response channel.  After the user
/// responds (or cancels), stores the resolved `JeuInfo` in `rom.jeu` and
/// transitions the bar to Packaging/waiting.
pub(crate) fn handle_wait_modal(
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
