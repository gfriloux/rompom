use std::sync::{Arc, Condvar, Mutex};

use crate::rom::Rom;

// ── Semaphore ─────────────────────────────────────────────────────────────

/// Everything `acquire()` decides on, behind one lock.
///
/// `cancelled` used to be an `AtomicBool` beside the mutex, which is why `acquire()`
/// had to wake up every 50 ms: a flag the condvar knows nothing about cannot be
/// notified, only polled. Under the same lock it is part of what `cvar.wait()` is
/// waiting for, and `cancel()` publishes it the same way `release()` publishes a permit.
struct SemaphoreState {
  available: usize,
  cancelled: bool,
}

/// A counting semaphore backed by a `Mutex` + `Condvar`.
///
/// Used for one purpose: `ss_sem` (capacity = `user_info.maxthreads`) limits how many
/// ScreenScraper calls are in flight at once. A second one used to serialise the
/// identification modal, until it turned out the render thread already does that — and
/// that capping it at one was what stopped more than one ROM from waiting at a time.
pub struct Semaphore {
  state: Mutex<SemaphoreState>,
  cvar: Condvar,
}

impl Semaphore {
  pub fn new(count: usize) -> Arc<Self> {
    Arc::new(Self {
      state: Mutex::new(SemaphoreState {
        available: count,
        cancelled: false,
      }),
      cvar: Condvar::new(),
    })
  }

  /// Acquires one permit. Returns `true` on success, `false` if cancelled.
  ///
  /// Sleeps until a permit is released or the run is cancelled — nothing here polls.
  /// A permit still wins over the cancelled flag: a worker that can proceed does,
  /// and finishes the step it is on rather than abandoning it half-done.
  pub fn acquire(&self) -> bool {
    let mut state = self.state.lock().unwrap();
    loop {
      if state.available > 0 {
        state.available -= 1;
        return true;
      }
      if state.cancelled {
        return false;
      }
      state = self.cvar.wait(state).unwrap();
    }
  }

  /// Releases one permit, unblocking a waiting caller if any.
  pub fn release(&self) {
    self.state.lock().unwrap().available += 1;
    self.cvar.notify_one();
  }

  /// Cancels all pending and future `acquire()` calls, causing them to
  /// return `false`. Idempotent.
  pub fn cancel(&self) {
    self.state.lock().unwrap().cancelled = true;
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

#[cfg(test)]
mod tests {
  use super::*;
  use std::{sync::mpsc, thread, time::Duration};

  // ── Semaphore ────────────────────────────────────────────────────────────

  /// Long enough that a loaded machine does not turn a passing test into a failing one:
  /// these assert that something happens at all, never how fast.
  const PATIENCE: Duration = Duration::from_secs(5);

  /// The nominal path: a permit handed back wakes exactly one caller waiting for it.
  #[test]
  fn a_released_permit_wakes_a_waiter() {
    let sem = Semaphore::new(1);
    assert!(sem.acquire(), "the only permit is free");

    let (tx, rx) = mpsc::channel();
    let waiter = Arc::clone(&sem);
    let thread = thread::spawn(move || tx.send(waiter.acquire()).unwrap());

    assert!(
      rx.recv_timeout(Duration::from_millis(100)).is_err(),
      "nothing has released a permit, so the waiter must still be parked"
    );
    sem.release();
    assert_eq!(rx.recv_timeout(PATIENCE), Ok(true));
    thread.join().unwrap();
  }

  /// Ctrl-C has to reach a worker parked on a saturated semaphore, and that is what the
  /// flag being under the same lock buys: `cancel()` publishes it and notifies, instead
  /// of leaving a 50 ms poll to notice it eventually.
  #[test]
  fn a_cancelled_waiter_wakes_up_and_says_it_got_nothing() {
    let sem = Semaphore::new(1);
    assert!(sem.acquire());

    let (tx, rx) = mpsc::channel();
    let waiter = Arc::clone(&sem);
    let thread = thread::spawn(move || tx.send(waiter.acquire()).unwrap());

    // Whether the waiter is already parked or has not got there yet, it must end up
    // refused: the flag it reads and the condvar it sleeps on are the same lock.
    sem.cancel();
    assert_eq!(rx.recv_timeout(PATIENCE), Ok(false));
    thread.join().unwrap();
  }

  /// Cancelling does not confiscate the permits that are still free — behaviour kept
  /// from the polling version, and deliberate. A worker that can proceed does, and
  /// finishes its step; refusing would turn work that was about to complete into work
  /// the next run has to redo.
  #[test]
  fn a_free_permit_still_wins_over_the_cancelled_flag() {
    let sem = Semaphore::new(1);
    sem.cancel();
    assert!(sem.acquire(), "the permit was free");
    assert!(!sem.acquire(), "and now there is nothing left to hand out");
  }
}
