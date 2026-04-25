use std::{
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
  },
  time::Duration,
};

use crate::rom::Rom;

// ── Semaphore ─────────────────────────────────────────────────────────────

/// A counting semaphore backed by a `Mutex` + `Condvar`.
///
/// Used for two purposes:
/// - `ss_sem` (capacity = `user_info.maxthreads`): limits concurrent SS API calls.
/// - `modal_sem` (capacity = 1): ensures at most one modal is open at a time.
pub struct Semaphore {
  available: Mutex<usize>,
  cvar: Condvar,
  cancelled: AtomicBool,
}

impl Semaphore {
  pub fn new(count: usize) -> Arc<Self> {
    Arc::new(Self {
      available: Mutex::new(count),
      cvar: Condvar::new(),
      cancelled: AtomicBool::new(false),
    })
  }

  /// Acquires one permit. Returns `true` on success, `false` if cancelled.
  /// Wakes up periodically to check the cancelled flag.
  pub fn acquire(&self) -> bool {
    let mut avail = self.available.lock().unwrap();
    loop {
      if *avail > 0 {
        *avail -= 1;
        return true;
      }
      if self.cancelled.load(Ordering::Relaxed) {
        return false;
      }
      let (guard, _) = self
        .cvar
        .wait_timeout(avail, Duration::from_millis(50))
        .unwrap();
      avail = guard;
    }
  }

  /// Releases one permit, unblocking a waiting caller if any.
  pub fn release(&self) {
    let mut avail = self.available.lock().unwrap();
    *avail += 1;
    self.cvar.notify_one();
  }

  /// Cancels all pending and future `acquire()` calls, causing them to
  /// return `false`. Idempotent.
  pub fn cancel(&self) {
    self.cancelled.store(true, Ordering::SeqCst);
    self.cvar.notify_all();
  }
}

// ── TaskQueue ─────────────────────────────────────────────────────────────

/// A reference to a specific step of a ROM, passed through the task queue.
pub type Task = (Arc<Mutex<Rom>>, usize);

struct QueueInner {
  /// Main LIFO stack — all steps except `WaitModal`.
  main: Vec<Task>,
  /// Blocking LIFO stack — `WaitModal` steps only.
  blocking: Vec<Task>,
  /// Set by `shutdown()` to signal workers to exit.
  shutdown: bool,
}

/// Two-stack LIFO task queue connecting the worker pools.
///
/// - `pool_main` workers call `pop_main()`.
/// - `pool_blocking` workers call `pop_blocking()`.
/// - Routing is automatic: `push()` reads `step.kind.is_blocking()` and
///   dispatches to the appropriate stack.
pub struct TaskQueue {
  inner: Mutex<QueueInner>,
  cvar_main: Condvar,
  cvar_blocking: Condvar,
}

impl TaskQueue {
  pub fn new() -> Arc<Self> {
    Arc::new(Self {
      inner: Mutex::new(QueueInner {
        main: Vec::new(),
        blocking: Vec::new(),
        shutdown: false,
      }),
      cvar_main: Condvar::new(),
      cvar_blocking: Condvar::new(),
    })
  }

  /// Enqueues a task, routing it to the correct stack based on the step kind.
  ///
  /// A `WaitModal` step with status `Skipped` is still routed to the main
  /// stack: it is a no-op and must not occupy a blocking worker.
  pub fn push(&self, rom: Arc<Mutex<Rom>>, step_index: usize) {
    let is_blocking = {
      let guard = rom.lock().unwrap();
      let step = &guard.pipeline[step_index];
      use crate::rom::StepStatus;
      step.kind.is_blocking() && step.status != StepStatus::Skipped
    };

    let mut inner = self.inner.lock().unwrap();
    if is_blocking {
      inner.blocking.push((rom, step_index));
      self.cvar_blocking.notify_one();
    } else {
      inner.main.push((rom, step_index));
      self.cvar_main.notify_one();
    }
  }

  /// Blocks until a non-blocking task is available, then returns it.
  /// Returns `None` when the queue has been shut down.
  pub fn pop_main(&self) -> Option<Task> {
    let mut inner = self.inner.lock().unwrap();
    loop {
      if let Some(task) = inner.main.pop() {
        return Some(task);
      }
      if inner.shutdown {
        return None;
      }
      inner = self.cvar_main.wait(inner).unwrap();
    }
  }

  /// Blocks until a blocking task is available, then returns it.
  /// Returns `None` when the queue has been shut down.
  pub fn pop_blocking(&self) -> Option<Task> {
    let mut inner = self.inner.lock().unwrap();
    loop {
      if let Some(task) = inner.blocking.pop() {
        return Some(task);
      }
      if inner.shutdown {
        return None;
      }
      inner = self.cvar_blocking.wait(inner).unwrap();
    }
  }

  /// Signals all blocked workers to exit.
  /// Should be called once all ROMs have been fully processed.
  pub fn shutdown(&self) {
    let mut inner = self.inner.lock().unwrap();
    inner.shutdown = true;
    self.cvar_main.notify_all();
    self.cvar_blocking.notify_all();
  }
}
