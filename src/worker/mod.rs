mod handlers;
mod helpers;
mod run_state;

pub use run_state::{
  apply_run_state, collect_run_state, load_run_state, restore_bar_for_resumed_rom, save_run_state,
  RunState,
};

use handlers::*;

use std::{
  any::Any,
  cell::RefCell,
  panic::{self, AssertUnwindSafe},
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

// ── Panic containment ──────────────────────────────────────────────────────

thread_local! {
  /// Where the most recent panic on this thread happened, recorded by the hook
  /// installed in `install_panic_hook`. `catch_unwind` gives us the payload but not
  /// the location, and a bare "index out of bounds" is not something you can act on.
  static LAST_PANIC_LOCATION: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Silences the default panic output for the duration of the run.
///
/// The default hook writes the message and backtrace straight to stderr. While
/// ratatui owns the terminal in raw mode that output lands on top of the interface
/// and corrupts it, and it is lost the moment the screen redraws — so it helps nobody.
/// The message is captured by `catch_unwind` instead and reported against the ROM that
/// caused it.
///
/// Panics outside a step handler (render thread, main) become silent as a result. They
/// were already illegible for the same reason, so nothing readable is lost.
pub fn install_panic_hook() {
  panic::set_hook(Box::new(|info| {
    let location = info
      .location()
      .map(|l| format!("{}:{}", l.file(), l.line()));
    LAST_PANIC_LOCATION.with(|cell| *cell.borrow_mut() = location);
  }));
}

/// Turns a caught panic payload into a message fit for `StepStatus::Failed`.
///
/// A panic carries either a `&'static str` (`panic!("boom")`) or a `String`
/// (`panic!("{}", x)`, and every `unwrap()` on an `Err`). Anything else is a custom
/// payload we can only name generically.
fn panic_message(payload: &(dyn Any + Send)) -> String {
  let what = payload
    .downcast_ref::<&'static str>()
    .map(|s| (*s).to_string())
    .or_else(|| payload.downcast_ref::<String>().cloned())
    .unwrap_or_else(|| "non-string panic payload".to_string());

  match LAST_PANIC_LOCATION.with(|cell| cell.borrow_mut().take()) {
    Some(location) => format!("panicked at {}: {}", location, what),
    None => format!("panicked: {}", what),
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
  // Unblock workers stuck in Semaphore::acquire() so they can exit cleanly.
  if ctx.interrupted.load(Ordering::Relaxed) {
    ctx.ss_sem.cancel();
    ctx.modal_sem.cancel();
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
  if ctx.interrupted.load(Ordering::Relaxed) {
    ctx.ss_sem.cancel();
    ctx.modal_sem.cancel();
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

  // A panicking handler used to take the whole run down with it: the worker thread
  // died, its ROM never reached SaveState, `remaining` never hit zero and the queue
  // never shut down — rompom hung with a half-drawn interface. Contain it here and
  // let the ROM fail on its own.
  let handler = AssertUnwindSafe(|| match kind {
    StepKind::ComputeHashes => handle_compute_hashes(&rom_arc, step_idx, ctx),
    StepKind::LookupSS => handle_lookup_ss(&rom_arc, step_idx, ctx),
    StepKind::WaitModal => handle_wait_modal(&rom_arc, step_idx, ctx),
    StepKind::BuildPackage => handle_build_package(&rom_arc, step_idx, ctx),
    StepKind::CopyRom => handle_copy_rom(&rom_arc, step_idx, ctx),
    StepKind::DownloadRom => handle_download_rom(&rom_arc, step_idx, ctx),
    StepKind::DownloadMedias => handle_download_medias(&rom_arc, step_idx, ctx),
    StepKind::SaveState => handle_save_state(&rom_arc, step_idx, ctx),
  });

  let (result, panicked) = match panic::catch_unwind(handler) {
    Ok(result) => (result, false),
    Err(payload) => {
      // Catching the unwind is only half the job. A handler almost always panics
      // while holding one of these locks, which poisons them — and every lock() in
      // this file unwraps, so the next one would panic in turn and we would be back
      // to a dead worker. Clearing the poison is what actually keeps the run alive.
      rom_arc.clear_poison();
      ctx.state.clear_poison();
      rom_arc.lock().unwrap().bar.clear_poison();
      (Err(panic_message(&*payload)), true)
    }
  };

  // Resolve final step status, handling retries.
  let final_status = match result {
    // Handler was cancelled mid-way (semaphore acquire returned false).
    // Reset to Pending so the step re-runs on resume; no dispatch.
    Err(ref msg) if msg == "interrupted" => {
      rom_arc.lock().unwrap().pipeline[step_idx].status = StepStatus::Pending;
      return;
    }
    Ok(s) => s,
    Err(msg) => {
      let (retry_count, max_retries) = {
        let rom = rom_arc.lock().unwrap();
        let step = &rom.pipeline[step_idx];
        (step.retry_count, step.max_retries())
      };
      // A panic is a bug, not a hiccup: an index out of bounds or an unwrap on None
      // will land the same way every time. Retrying it only spends the backoff delay
      // to reach the same failure, so panics go straight to Failed.
      if !panicked && retry_count < max_retries {
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

#[cfg(test)]
mod tests {
  use super::*;

  /// `unwrap()` on a None/Err — by far the most common way a handler dies — produces a
  /// String payload, while `panic!("literal")` produces a &'static str. Both have to
  /// reach StepStatus::Failed, or the ROM fails with no explanation.
  #[test]
  fn panic_message_extracts_both_payload_kinds() {
    let payload = panic::catch_unwind(|| panic!("boom")).unwrap_err();
    assert!(panic_message(&*payload).contains("boom"));

    let payload = panic::catch_unwind(|| panic!("{} is missing", "sha1")).unwrap_err();
    assert!(panic_message(&*payload).contains("sha1 is missing"));

    // How a step handler actually dies: indexing a pipeline or a media list short.
    let payload = panic::catch_unwind(|| {
      let steps: Vec<u8> = Vec::new();
      steps[3]
    })
    .unwrap_err();
    assert!(panic_message(&*payload).contains("index out of bounds"));
  }

  /// The hook records the location so the failure names a file and line rather than
  /// just "index out of bounds", which is not actionable on its own.
  #[test]
  fn panic_message_includes_the_recorded_location() {
    LAST_PANIC_LOCATION.with(|cell| *cell.borrow_mut() = Some("src/rom/mod.rs:42".to_string()));
    let payload = panic::catch_unwind(|| panic!("boom")).unwrap_err();
    assert_eq!(
      panic_message(&*payload),
      "panicked at src/rom/mod.rs:42: boom"
    );
  }

  /// The location is taken, not copied: a later failure with no recorded location must
  /// not inherit the previous one and point at innocent code.
  #[test]
  fn panic_message_does_not_reuse_a_stale_location() {
    LAST_PANIC_LOCATION.with(|cell| *cell.borrow_mut() = Some("src/first.rs:1".to_string()));
    let payload = panic::catch_unwind(|| panic!("first")).unwrap_err();
    assert!(panic_message(&*payload).contains("src/first.rs:1"));

    let payload = panic::catch_unwind(|| panic!("second")).unwrap_err();
    let msg = panic_message(&*payload);
    assert!(
      !msg.contains("src/first.rs"),
      "stale location leaked into {msg:?}"
    );
  }

  /// This is the half of the fix that is easy to miss. Catching the unwind is not
  /// enough: the handler panicked while holding the lock, so every later
  /// `lock().unwrap()` would panic in turn and the worker would die anyway.
  #[test]
  fn clearing_poison_makes_a_mutex_usable_again() {
    let shared = Arc::new(Mutex::new(0u8));

    let inner = Arc::clone(&shared);
    let caught = panic::catch_unwind(AssertUnwindSafe(|| {
      let _guard = inner.lock().unwrap();
      panic!("died holding the lock");
    }));
    assert!(caught.is_err());

    assert!(
      shared.lock().is_err(),
      "the mutex should be poisoned at this point"
    );

    shared.clear_poison();
    assert!(
      shared.lock().is_ok(),
      "after clear_poison the worker must be able to carry on"
    );
  }
}
