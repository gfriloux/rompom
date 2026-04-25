use std::{
  collections::HashMap,
  fs,
  io::Write as _,
  sync::{atomic::Ordering, Arc, Mutex},
};

use crate::{
  package::Medias,
  rom::{Rom, StepStatus},
  state::RomStateEntry,
};

use super::super::WorkerContext;

// ── Helper ────────────────────────────────────────────────────────────────

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

// ── SaveState ─────────────────────────────────────────────────────────────

/// Record this ROM's `RomStateEntry` into the shared in-memory `SystemState`,
/// signal the UI, and shut down the queue once all ROMs are done.
///
/// The state file is flushed to disk once by `main.rs` after all workers join.
pub(crate) fn handle_save_state(
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
