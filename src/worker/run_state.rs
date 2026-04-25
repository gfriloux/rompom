use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::rom::{Rom, StepStatus};

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
