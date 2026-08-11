use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::{
  rom::{Rom, Step, StepStatus},
  state::write_with_rotation,
};

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
///
/// Through the same write-rename as `state.yml`: this file is written while the process
/// is already shutting down, and a truncated one is worse than none — the resume prompt
/// would offer to continue a run whose statuses it cannot read.
pub fn save_run_state(system_name: &str, state: &RunState) -> std::io::Result<()> {
  let path = format!("{}.run.yml", system_name);
  let yaml = serde_yaml::to_string(state).map_err(std::io::Error::other)?;
  write_with_rotation(&path, &yaml)
}

/// Load a `RunState` from disk.
pub fn load_run_state(path: &str) -> Result<RunState, Box<dyn std::error::Error>> {
  let content = std::fs::read_to_string(path)?;
  let state = serde_yaml::from_str(&content)?;
  Ok(state)
}

/// Apply a previously saved run state to an already-constructed ROM.
///
/// A ROM is resumed **all or nothing**: either its pipeline finished in the previous run
/// and every status is restored, or it starts over from the beginning.
///
/// Partial resumption is not possible, and the reason is that every step feeds the next
/// ones through the `Rom` struct, not through disk:
///
/// | step | leaves behind in memory |
/// |---|---|
/// | `ComputeHashes` | `sha1`, `md5`, `crc32`, `rom_unchanged` |
/// | `LookupSS` / `WaitModal` | `jeu` |
/// | `BuildPackage` | `medias`, `romname`, `package_unchanged` |
///
/// `run.yml` records step statuses only. A resumed `Rom` is a fresh struct, so all of
/// that is `None`. Restoring `LookupSS` as Done and letting `BuildPackage` run meant
/// building a package with no `JeuInfo` at all: an empty description.xml overwriting the
/// good one, and a `pkgver` bumped for the privilege. Further along, `SaveState` would
/// persist `ss_game_id: None` and an empty media map, throwing away the cache that makes
/// the next run fast.
///
/// Starting over is affordable because every expensive operation is already
/// skip-if-valid: `ComputeHashes` has the mtime+size fast path, `LookupSS` reuses the
/// cached `ss_game_id`, downloads verify sha1 before fetching, and `BuildPackage` only
/// rewrites when something actually changed. What a resumed ROM re-does is checks, not
/// work.
///
/// One thing is genuinely lost: a game identified by hand through the modal has to be
/// identified again, because that choice only ever lived in `rom.jeu` and in the
/// `state.yml` entry `SaveState` never got to write.
///
/// Returns `true` when the ROM was already finished, so the caller can mark it as such
/// and keep the run's `remaining` counter in step with what `main` counted.
pub fn apply_run_state(rom: &mut Rom, run_entry: &RunRomEntry) {
  rom.finished = restore_finished_pipeline(&mut rom.pipeline, run_entry);
}

/// Takes the pipeline rather than the `Rom` so it can be tested without building a
/// `RomBar`, and with it the whole terminal UI.
fn restore_finished_pipeline(pipeline: &mut [Step], run_entry: &RunRomEntry) -> bool {
  // The leaf is the last step of both DAGs; if it reached a terminal status the ROM is
  // done and nothing may run again.
  let leaf_finished = run_entry
    .step_statuses
    .get(pipeline.len() - 1)
    .is_some_and(|s| {
      matches!(
        s,
        RunStepStatus::Done | RunStepStatus::Skipped | RunStepStatus::Failed(_)
      )
    });

  if !leaf_finished {
    // Leave the pipeline exactly as constructed: everything Pending, WaitModal Skipped.
    // No wait_for is touched, so the counters cannot underflow later.
    return false;
  }

  for (idx, saved) in run_entry.step_statuses.iter().enumerate() {
    if idx >= pipeline.len() {
      break;
    }
    pipeline[idx].status = match saved {
      RunStepStatus::Done => StepStatus::Done,
      RunStepStatus::Skipped => StepStatus::Skipped,
      RunStepStatus::Failed(e) => StepStatus::Failed(e.clone()),
      // A step still running when the interrupt landed cannot have produced anything
      // downstream, yet the leaf is finished — treat it as skipped rather than leave it
      // Pending, which would re-enqueue it in a pipeline nothing else will follow.
      RunStepStatus::Pending | RunStepStatus::InProgress => StepStatus::Skipped,
    };
  }

  true
}

/// Update the ROM's UI bar to reflect its restored pipeline state.
///
/// Must be called after `apply_run_state`. Since resumption is all or nothing there are
/// only two cases: a ROM that finished last run goes straight to the Completed panel,
/// and any other ROM restarts from the first step, which is the "queued / Discovering"
/// state `new_rom_bar()` already set.
pub fn restore_bar_for_resumed_rom(rom: &Rom) {
  // Look for the failure anywhere in the pipeline, not just on the leaf. A ROM cut
  // short upstream has its failure on the step that broke and `Skipped` everywhere
  // after it — including the leaf — so reading the leaf alone restored it into the
  // Completed panel as a success.
  if let Some(cause) = failure_cause(&rom.pipeline) {
    rom.bar.finish_error(&cause);
    return;
  }

  let leaf = &rom.pipeline[rom.pipeline.len() - 1];
  if matches!(leaf.status, StepStatus::Done | StepStatus::Skipped) {
    rom.bar.restored();
    rom.bar.finish(false);
  }
}

/// The cause carried by the first failed step of a pipeline, if any.
fn failure_cause(pipeline: &[Step]) -> Option<String> {
  pipeline.iter().find_map(|step| match &step.status {
    StepStatus::Failed(cause) => Some(cause.clone()),
    _ => None,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::rom::{StepData, StepKind};

  /// Same shape as the folder DAG: 7 steps, WaitModal Skipped by default, SaveState
  /// the single leaf waiting on two predecessors.
  fn pipeline() -> Vec<Step> {
    vec![
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
    ]
  }

  fn entry(statuses: Vec<RunStepStatus>) -> RunRomEntry {
    RunRomEntry {
      filename: "Sonic.zip".to_string(),
      step_statuses: statuses,
    }
  }

  /// The corruption case. Interrupted between LookupSS (Done) and BuildPackage
  /// (Pending): restoring LookupSS as Done let BuildPackage run with `jeu` at None,
  /// writing an empty description.xml over the good one and bumping pkgver for it.
  /// The whole pipeline has to start over instead.
  #[test]
  fn an_unfinished_rom_restarts_from_the_beginning() {
    use RunStepStatus as R;
    let mut p = pipeline();

    let finished = restore_finished_pipeline(
      &mut p,
      &entry(vec![
        R::Done,    // ComputeHashes
        R::Done,    // LookupSS  ← produced `jeu`, which is gone
        R::Skipped, // WaitModal
        R::Pending, // BuildPackage
        R::Pending, // CopyRom
        R::Pending, // DownloadMedias
        R::Pending, // SaveState
      ]),
    );

    assert!(!finished);
    assert_eq!(
      p[0].status,
      StepStatus::Pending,
      "ComputeHashes must re-run"
    );
    assert_eq!(p[1].status, StepStatus::Pending, "LookupSS must re-run");
    assert_eq!(
      p[2].status,
      StepStatus::Skipped,
      "WaitModal keeps its default"
    );
    assert_eq!(p[3].status, StepStatus::Pending);
  }

  /// Nothing may be decremented for a restarting ROM: the counters have to stay at
  /// their constructed values or do_dispatch would underflow them during the run.
  #[test]
  fn restarting_leaves_every_wait_for_untouched() {
    use RunStepStatus as R;
    let mut p = pipeline();
    let before: Vec<usize> = p.iter().map(|s| s.wait_for_count()).collect();

    restore_finished_pipeline(
      &mut p,
      &entry(vec![
        R::Done,
        R::Done,
        R::Skipped,
        R::InProgress,
        R::Pending,
        R::Pending,
        R::Pending,
      ]),
    );

    let after: Vec<usize> = p.iter().map(|s| s.wait_for_count()).collect();
    assert_eq!(before, after);
    assert_eq!(after[6], 2, "SaveState still waits on both branches");
  }

  /// A ROM that finished keeps its statuses so it shows up in the Completed panel, and
  /// reports itself finished — `main` excludes it from `remaining`, so a later
  /// decrement would underflow the counter and the queue would never shut down.
  #[test]
  fn a_finished_rom_is_restored_and_reports_itself_finished() {
    use RunStepStatus as R;
    let mut p = pipeline();

    let finished = restore_finished_pipeline(
      &mut p,
      &entry(vec![
        R::Done,
        R::Done,
        R::Skipped,
        R::Done,
        R::Done,
        R::Done,
        R::Done,
      ]),
    );

    assert!(finished);
    assert_eq!(p[6].status, StepStatus::Done);
    assert!(p.iter().all(|s| s.status != StepStatus::Pending));
  }

  /// A ROM that failed last run is finished too: its leaf was cut to Skipped by
  /// skip_successors, and it must not be retried silently inside the same resume.
  #[test]
  fn a_rom_cut_short_by_a_failure_counts_as_finished() {
    use RunStepStatus as R;
    let mut p = pipeline();

    let finished = restore_finished_pipeline(
      &mut p,
      &entry(vec![
        R::Done,
        R::Done,
        R::Skipped,
        R::Done,
        R::Failed("download failed".to_string()),
        R::Done,
        R::Skipped,
      ]),
    );

    assert!(finished);
    assert!(matches!(p[4].status, StepStatus::Failed(_)));
    assert!(p.iter().all(|s| s.status != StepStatus::Pending));
  }

  /// The failure sits on the step that broke, and everything after it — the leaf
  /// included — is Skipped. Reading the leaf alone therefore said "Done" and restored a
  /// failed ROM into the Completed panel as a success, cause and all lost.
  #[test]
  fn a_resumed_failure_is_found_off_the_leaf() {
    use RunStepStatus as R;
    let mut p = pipeline();

    restore_finished_pipeline(
      &mut p,
      &entry(vec![
        R::Done,
        R::Done,
        R::Skipped,
        R::Done,
        R::Failed("daily ScreenScraper scrape quota exceeded".to_string()),
        R::Done,
        R::Skipped,
      ]),
    );

    assert!(matches!(p.last().unwrap().status, StepStatus::Skipped));
    assert_eq!(
      failure_cause(&p).as_deref(),
      Some("daily ScreenScraper scrape quota exceeded")
    );
  }

  /// A ROM that simply finished must not be reported as failed.
  #[test]
  fn a_clean_pipeline_has_no_cause() {
    use RunStepStatus as R;
    let mut p = pipeline();
    restore_finished_pipeline(
      &mut p,
      &entry(vec![
        R::Done,
        R::Done,
        R::Skipped,
        R::Done,
        R::Done,
        R::Done,
        R::Done,
      ]),
    );
    assert_eq!(failure_cause(&p), None);
  }

  /// A step still in flight when the interrupt landed, in a pipeline whose leaf did
  /// finish, must not be left Pending — it would be re-enqueued on its own with nothing
  /// downstream to follow it.
  #[test]
  fn an_in_flight_step_in_a_finished_pipeline_is_not_requeued() {
    use RunStepStatus as R;
    let mut p = pipeline();

    restore_finished_pipeline(
      &mut p,
      &entry(vec![
        R::Done,
        R::Done,
        R::Skipped,
        R::Done,
        R::InProgress,
        R::Done,
        R::Done,
      ]),
    );

    assert_eq!(p[4].status, StepStatus::Skipped);
    assert!(p.iter().all(|s| s.status != StepStatus::Pending));
  }
}
