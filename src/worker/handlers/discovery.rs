use std::{
  fs,
  sync::{Arc, Mutex},
  time::UNIX_EPOCH,
};

use crate::{
  hash::{crc32_file, md5_file, sha1_file},
  rom::{Rom, RomSource, StepData, StepError, StepKind, StepStatus},
  ui::{ModalCandidate, ModalRequest, ModalResponse},
};

use super::super::{
  helpers::{candidate_from, lookup_failure, search_name, NAME_REGIONS},
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
) -> Result<StepStatus, StepError> {
  let (filename, local_path, extra_disc_paths) = {
    let rom = rom_arc.lock().unwrap();
    let path = match &rom.source.source {
      RomSource::Folder(f) => f.local_path.clone(),
      _ => unreachable!("ComputeHashes only runs on folder sources"),
    };
    // Collect extra disc local paths for hash computation below.
    let extra_paths: Vec<std::path::PathBuf> = rom
      .source
      .extra_discs
      .iter()
      .map(|d| d.local_path.clone().unwrap_or_default())
      .collect();
    (rom.source.filename.clone(), path, extra_paths)
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
    let sha1 = sha1_file(&local_path).map_err(StepError::transient)?;
    let md5 = md5_file(&local_path).map_err(StepError::transient)?;
    let crc32 = crc32_file(&local_path).map_err(StepError::transient)?;
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

  {
    let rom = rom_arc.lock().unwrap();
    rom.bar.set_hashes(rom.sha1.clone(), rom.size);
  }

  // ── Compute sha1 for extra discs (no fast-path for multi-disc extras) ────
  let extra_disc_sha1s: Vec<String> = extra_disc_paths
    .iter()
    .map(|p| sha1_file(p))
    .collect::<Result<_, _>>()
    .map_err(StepError::transient)?;

  if !extra_disc_sha1s.is_empty() {
    rom_arc.lock().unwrap().extra_disc_sha1s = extra_disc_sha1s.clone();
  }

  // ── Check if ROM is unchanged based on saved state ────────────────────
  let sha1_now: Option<String> = rom_arc.lock().unwrap().sha1.clone();
  let (unchanged, state_rom_sha1) = {
    let state = ctx.state.lock().unwrap();
    match state.roms.get(&filename) {
      None => (None, None),
      Some(entry) => {
        let current = sha1_now.as_deref().unwrap_or("");
        let disc1_ok = !current.is_empty() && entry.rom_sha1 == current;
        // Also verify extra discs match in count and sha1.
        let extras_ok = entry.extra_disc_sha1s.len() == extra_disc_sha1s.len()
          && entry
            .extra_disc_sha1s
            .iter()
            .zip(&extra_disc_sha1s)
            .all(|(saved, cur)| !cur.is_empty() && saved == cur);
        (Some(disc1_ok && extras_ok), Some(entry.rom_sha1.clone()))
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
) -> Result<StepStatus, StepError> {
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
    // Collect extra-disc sha1s from the source (seeded at collection time).
    let extra_disc_sha1s_current: Vec<String> = {
      let rom = rom_arc.lock().unwrap();
      rom.extra_disc_sha1s.clone()
    };

    let (unchanged, state_rom_sha1) = {
      let state = ctx.state.lock().unwrap();
      match state.roms.get(&filename) {
        None => (None, None),
        Some(entry) => {
          let current = sha1.as_deref().unwrap_or("");
          let disc1_ok = !current.is_empty() && entry.rom_sha1 == current;
          // Extra discs must match in count and sha1.
          let extras_ok = entry.extra_disc_sha1s.len() == extra_disc_sha1s_current.len()
            && entry
              .extra_disc_sha1s
              .iter()
              .zip(&extra_disc_sha1s_current)
              .all(|(saved, current)| !current.is_empty() && saved == current);
          (Some(disc1_ok && extras_ok), Some(entry.rom_sha1.clone()))
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
  if !ctx.ss_sem.acquire() {
    return Err(StepError::Interrupted);
  }
  let lookup = if let Some(gid) = cached_game_id {
    ctx.ss.jeuinfo_by_gameid(ctx.system.id, gid)
  } else {
    ctx
      .ss
      .jeuinfo(ctx.system.id, &filename, size, crc32, md5, sha1)
  };
  ctx.ss_sem.release();

  // A failed lookup used to be flattened to None by `.ok()`, which meant "ScreenScraper
  // does not know this game" and opened the identification modal. Only a 404 means that.
  let ji = match lookup {
    Ok(jeu) => Some(jeu),
    Err(e) => match lookup_failure(e.failure()) {
      Some(step_error) => return Err(step_error),
      None => None,
    },
  };

  if let Some(jeu) = ji {
    // ── Found ─────────────────────────────────────────────────────────
    let name = jeu.find_name(NAME_REGIONS);
    {
      let mut rom = rom_arc.lock().unwrap();
      rom.jeu = Some(jeu);
      rom.bar.found(&name);
    }
    // WaitModal will be Skipped: the ROM is now queued for packaging.
    rom_arc.lock().unwrap().bar.queued_for_packaging();
    Ok(StepStatus::Done)
  } else {
    // ── Not found: run jeu_recherche and hand off to WaitModal ────────
    if !ctx.ss_sem.acquire() {
      return Err(StepError::Interrupted);
    }
    let search = ctx
      .ss
      .jeu_recherche(Some(ctx.system.id), &search_name(&filename));
    ctx.ss_sem.release();

    // `unwrap_or_default()` turned a failed search into zero candidates, so the user
    // got an empty modal and no idea why. A search that finds nothing legitimately
    // returns Ok(vec![]) — that is still an empty modal, but an honest one.
    let search_results = match search {
      Ok(results) => results,
      Err(e) => match lookup_failure(e.failure()) {
        Some(step_error) => return Err(step_error),
        None => Vec::new(),
      },
    };

    let lang_refs: Vec<&str> = ctx.lang.iter().map(|s| s.as_str()).collect();
    let display_candidates: Vec<ModalCandidate> = search_results
      .iter()
      .map(|j| candidate_from(j, &lang_refs))
      .collect();

    // Store candidates in this step's data and unlock WaitModal.
    {
      let mut rom = rom_arc.lock().unwrap();
      rom.bar.set_candidates(
        display_candidates.len(),
        display_candidates.first().map(|c| c.name.clone()),
      );
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
/// Sends a `ModalRequest` and blocks on the response channel. The request is parked by
/// the render thread until the user gets to it, so several ROMs can be waiting at once —
/// each one holding a blocking-pool worker for as long as it waits. After the user
/// responds (or cancels), stores the resolved `JeuInfo` in `rom.jeu`.
///
/// Nothing serialises the modals here: there is one render thread and one screen, so at
/// most one modal can be open whatever the workers do.
pub(crate) fn handle_wait_modal(
  rom_arc: &Arc<Mutex<Rom>>,
  _step_idx: usize,
  ctx: &WorkerContext,
) -> Result<StepStatus, StepError> {
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

  // Nobody is watching in plain mode, so there is no one to answer the modal. Fail the
  // ROM and let the run carry on: it lands in the Completed log as an error and in the
  // Failures section of the summary, with the cause saying what to do about it.
  //
  // Not identified-by-guess: packaging a ROM on the first search hit writes a wrong
  // description.xml, bumps its pkgver, and persists a wrong ss_game_id that every later
  // run then trusts — the exact damage P1.1 closed.
  if crate::ui::is_plain() {
    return Err(StepError::Fatal(format!(
      "not identified — needs manual identification, and {} candidate(s) cannot be shown \
       without a terminal",
      candidates.len()
    )));
  }

  // Signal the UI that we're waiting for user input.
  rom_arc.lock().unwrap().bar.waiting_for_user();

  let (resp_tx, resp_rx) = crossbeam_channel::bounded::<ModalResponse>(1);
  let ss_for_closure = Arc::clone(&ctx.ss);
  let system_id = ctx.system.id;

  ctx
    .modal_tx
    .send(ModalRequest {
      row: rom_arc.lock().unwrap().bar.row(),
      filename: filename.clone(),
      sha1: sha1_opt,
      candidates,
      response: resp_tx,
      // Called by the TUI render thread to show a confirmation after manual ID entry.
      fetch_by_id: Box::new(move |game_id| {
        ss_for_closure
          .jeuinfo_by_gameid(system_id, game_id)
          .map(|j| j.find_name(NAME_REGIONS).to_string())
          .map_err(|e| match lookup_failure(e.failure()) {
            // 404 — the ID is wrong, and retyping it is exactly the right move.
            None => "ID not found on ScreenScraper".to_string(),
            Some(step_error) => step_error.to_string(),
          })
      }),
    })
    .map_err(|e| StepError::Fatal(format!("modal channel closed: {}", e)))?;

  let response = resp_rx
    .recv()
    .map_err(|_| StepError::Fatal("modal response channel closed".to_string()))?;

  // ── Resolve JeuInfo from the user's response ───────────────────────────
  //
  // Cancelling is a decision: `None` means "package this ROM without metadata".
  // A failed fetch is not — swallowing it here would discard the identification the
  // user just typed, write an empty description.xml over a good one and bump pkgver
  // for it. Ctrl-C mid-fetch used to land in the same hole, via `return None`.
  let jeu = match response {
    ModalResponse::SelectedId(id) | ModalResponse::ManualId(id) => match id.parse::<u32>() {
      Err(_) => None,
      Ok(gid) => {
        if !ctx.ss_sem.acquire() {
          return Err(StepError::Interrupted);
        }
        let result = ctx.ss.jeuinfo_by_gameid(ctx.system.id, gid);
        ctx.ss_sem.release();
        match result {
          Ok(j) => Some(j),
          Err(e) => {
            return Err(lookup_failure(e.failure()).unwrap_or_else(|| {
              StepError::Fatal(format!("ScreenScraper does not know game ID {}", gid))
            }))
          }
        }
      }
    },
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

  // Queued for packaging regardless of found/cancelled.
  rom_arc.lock().unwrap().bar.queued_for_packaging();

  Ok(StepStatus::Done)
}
