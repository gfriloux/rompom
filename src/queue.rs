use std::{
  sync::{Arc, Condvar, Mutex},
  time::{Duration, Instant},
};

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

// ── Delayed lane ──────────────────────────────────────────────────────────

/// Tasks waiting out a retry backoff, kept latest-first so the next one due is at the end.
///
/// Generic over the payload for the same reason `skip_successors` takes a `&mut [Step]`:
/// a `Task` holds a `Rom`, a `Rom` holds a `RomBar`, and a `RomBar` cannot be built
/// without the whole terminal interface. Left open, the scheduling arithmetic — which is
/// all there is to get wrong here — can be tested on a list of strings.
struct Delayed<T> {
  entries: Vec<(Instant, T)>,
}

impl<T> Delayed<T> {
  fn new() -> Self {
    Self {
      entries: Vec::new(),
    }
  }

  /// Files a task under the instant it becomes runnable.
  fn insert(&mut self, due: Instant, task: T) {
    let pos = self.entries.partition_point(|(d, _)| *d > due);
    self.entries.insert(pos, (due, task));
  }

  /// Removes and returns everything due at `now`, earliest deadline first.
  fn take_due(&mut self, now: Instant) -> Vec<T> {
    let mut due = Vec::new();
    while self.entries.last().is_some_and(|(at, _)| *at <= now) {
      due.extend(self.entries.pop().map(|(_, task)| task));
    }
    due
  }

  /// When the next task falls due, or `None` when none is waiting.
  fn next_due(&self) -> Option<Instant> {
    self.entries.last().map(|(at, _)| *at)
  }
}

struct QueueInner {
  /// Main LIFO stack — all steps except `WaitModal`.
  main: Vec<Task>,
  /// Blocking LIFO stack — `WaitModal` steps only.
  blocking: Vec<Task>,
  /// Steps waiting out a retry backoff before they rejoin `main`.
  delayed: Delayed<Task>,
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
        delayed: Delayed::new(),
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

  /// Enqueues a task that must not run before `delay` has elapsed — a step waiting out
  /// its retry backoff.
  ///
  /// The wait belongs here rather than in the worker that failed. A worker parked in
  /// `thread::sleep` for up to sixteen seconds is one that does none of the other ROMs'
  /// work, and one that Ctrl-C cannot reach: `shutdown()` and `cancel()` both wake
  /// threads that are waiting *on something*, and a sleeping thread waits on nothing.
  pub fn push_after(&self, rom: Arc<Mutex<Rom>>, step_index: usize, delay: Duration) {
    debug_assert!(
      !rom.lock().unwrap().pipeline[step_index].kind.is_blocking(),
      "a delayed task always rejoins the main stack, so a blocking step must never take \
       this path"
    );
    let due = Instant::now() + delay;
    let mut inner = self.inner.lock().unwrap();
    inner.delayed.insert(due, (rom, step_index));
    // Wakes one popper so it can arm its own timeout: workers already parked are waiting
    // without one, and nothing else is going to come and tell them about this deadline.
    self.cvar_main.notify_one();
  }

  /// Blocks until a non-blocking task is available, then returns it.
  /// Returns `None` when the queue has been shut down.
  pub fn pop_main(&self) -> Option<Task> {
    let mut inner = self.inner.lock().unwrap();
    loop {
      let now = Instant::now();
      let ready = inner.delayed.take_due(now);
      inner.main.extend(ready);

      if let Some(task) = inner.main.pop() {
        return Some(task);
      }
      if inner.shutdown {
        // Anything still waiting out a backoff is dropped, deliberately: those steps are
        // `Pending`, so `run.yml` records them and the resume replays them. Sitting on a
        // deadline nobody will act on would only hold up the exit — which is the whole
        // reason the sleep moved in here.
        return None;
      }
      inner = match inner.delayed.next_due() {
        Some(due) => {
          let (guard, _) = self
            .cvar_main
            .wait_timeout(inner, due.saturating_duration_since(now))
            .unwrap();
          guard
        }
        None => self.cvar_main.wait(inner).unwrap(),
      };
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

  /// Reopens a queue that has been shut down, for another round of work.
  ///
  /// The three lanes are emptied first. `shutdown()` deliberately abandons whatever is
  /// waiting out a backoff, and by the time this is called every worker of the previous
  /// round has joined and `remaining` is zero — so anything still filed here belongs to a
  /// ROM that already reached its leaf, and is stale by construction. The next round's
  /// only source of tasks is the steps the caller is about to push.
  pub fn restart(&self) {
    let mut inner = self.inner.lock().unwrap();
    inner.main.clear();
    inner.blocking.clear();
    inner.delayed = Delayed::new();
    inner.shutdown = false;
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

  // ── Delayed lane ─────────────────────────────────────────────────────────

  /// A backoff that has not elapsed hands nothing back, and says when to come again.
  #[test]
  fn a_task_is_not_due_before_its_deadline() {
    let now = Instant::now();
    let mut delayed = Delayed::new();
    delayed.insert(now + Duration::from_secs(4), "retry");

    assert!(delayed.take_due(now).is_empty());
    assert_eq!(delayed.next_due(), Some(now + Duration::from_secs(4)));
    assert_eq!(
      delayed.take_due(now + Duration::from_secs(4)),
      vec!["retry"]
    );
    assert_eq!(delayed.next_due(), None);
  }

  /// The deadline is reached, not passed: `1 << retry_count` seconds after the failure
  /// the step is runnable, and a strict comparison would hold it for another wakeup.
  #[test]
  fn a_deadline_falling_exactly_now_is_due() {
    let now = Instant::now();
    let mut delayed = Delayed::new();
    delayed.insert(now, "retry");
    assert_eq!(delayed.take_due(now), vec!["retry"]);
  }

  /// Insertion order says nothing about who runs first. Backoffs double, so a step on
  /// its fourth attempt is filed eight seconds behind one on its first, whichever
  /// failed first.
  #[test]
  fn deadlines_come_back_earliest_first_whatever_the_insertion_order() {
    let now = Instant::now();
    let mut delayed = Delayed::new();
    delayed.insert(now + Duration::from_secs(8), "third");
    delayed.insert(now + Duration::from_secs(1), "first");
    delayed.insert(now + Duration::from_secs(4), "second");

    assert_eq!(delayed.next_due(), Some(now + Duration::from_secs(1)));
    assert_eq!(
      delayed.take_due(now + Duration::from_secs(60)),
      vec!["first", "second", "third"]
    );
  }

  // ── TaskQueue ────────────────────────────────────────────────────────────

  /// A worker parked on an empty queue has to come back when the run ends — including
  /// now that the wait can carry a timeout.
  #[test]
  fn shutdown_releases_a_parked_worker() {
    let queue = TaskQueue::new();
    let (tx, rx) = mpsc::channel();
    let popper = Arc::clone(&queue);
    let thread = thread::spawn(move || tx.send(popper.pop_main().is_none()).unwrap());

    queue.shutdown();
    assert_eq!(rx.recv_timeout(PATIENCE), Ok(true));
    thread.join().unwrap();
  }

  /// A retry runs a fresh pool against the same queue. Without `restart()` every worker
  /// of the second round would find `shutdown` still set, pop nothing and exit — the
  /// re-armed ROM would sit there, and the report would come back claiming it had run.
  #[test]
  fn a_restarted_queue_parks_a_worker_again() {
    let queue = TaskQueue::new();
    queue.shutdown();
    assert!(
      queue.pop_main().is_none(),
      "precondition: the queue is shut"
    );

    queue.restart();
    let (tx, rx) = mpsc::channel();
    let popper = Arc::clone(&queue);
    let thread = thread::spawn(move || tx.send(popper.pop_main().is_none()).unwrap());

    assert!(
      rx.recv_timeout(Duration::from_millis(100)).is_err(),
      "the queue is open again, so the worker must park rather than exit"
    );
    queue.shutdown();
    assert_eq!(rx.recv_timeout(PATIENCE), Ok(true));
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
