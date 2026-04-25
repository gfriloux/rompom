use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::rom::{Rom, StepKind, StepStatus};

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

    // Only propagate through the DAG if this step was actually reached in the
    // previous run, i.e. its own wait_for has already reached 0 (all its
    // predecessors were restored as Done/Skipped earlier in this loop).
    //
    // Without this guard, a step that is Skipped by default (WaitModal) but
    // whose predecessor is still Pending would incorrectly decrement its
    // successor's wait_for — causing an underflow when the predecessor later
    // dispatches the same step via do_dispatch during the resumed run.
    if rom.pipeline[idx].wait_for_count() != 0 {
      continue;
    }

    let nexts: Vec<usize> = rom.pipeline[idx].next.clone();
    for next_idx in nexts {
      rom.pipeline[next_idx].dec_wait_for();
    }
  }
}

/// Update the ROM's UI bar to reflect its restored pipeline state.
///
/// Must be called after `apply_run_state`. Places each ROM in the correct
/// panel so the user sees:
/// - already-completed ROMs in the Completed panel,
/// - ROMs waiting for downloads in the Downloads panel,
/// - ROMs waiting for packaging in the Discovery panel (Packaging sub-phase),
/// - ROMs still in Discovery unchanged (default "queued" state).
pub fn restore_bar_for_resumed_rom(rom: &Rom) {
  let pipeline = &rom.pipeline;
  let last = pipeline.len() - 1;

  // ROM was fully completed in the previous run.
  match &pipeline[last].status {
    StepStatus::Done | StepStatus::Skipped => {
      rom.bar.finish(false);
      return;
    }
    StepStatus::Failed(_) => {
      rom.bar.finish_error();
      return;
    }
    _ => {}
  }

  // Find the first step that still needs to run to infer the current phase.
  let first_pending = pipeline
    .iter()
    .find(|s| s.status == StepStatus::Pending)
    .map(|s| &s.kind);

  match first_pending {
    Some(StepKind::BuildPackage) => {
      rom.bar.preparing_pending();
    }
    Some(
      StepKind::CopyRom | StepKind::DownloadRom | StepKind::DownloadMedias | StepKind::SaveState,
    ) => {
      rom.bar.downloading_pending();
    }
    // ComputeHashes / LookupSS / WaitModal → still in Discovering phase.
    // The bar was initialised to "queued / Discovering" in new_rom_bar(); no change needed.
    _ => {}
  }
}
