mod handlers;
mod helpers;
mod run_state;

pub use run_state::{apply_run_state, collect_run_state, load_run_state, save_run_state, RunState};

use handlers::*;

use std::{
  sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
  },
  time::Instant,
};

use screenscraper::ScreenScraper;

use crate::{
  conf::System,
  queue::{Semaphore, TaskQueue},
  rom::{Rom, StepKind, StepStatus},
  state::SystemState,
  ui::ModalRequest,
};

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
