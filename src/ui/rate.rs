//! Throughput over a sliding window: the sparkline, the ROM/min and MiB/s figures, and
//! the ETA.
//!
//! Everything is keyed on **elapsed time**, a `Duration` the caller passes in, rather
//! than on `Instant::now()`. An `Instant` cannot be built at an arbitrary point, so a
//! window driven by one is a window no test can walk through.

use std::{collections::VecDeque, time::Duration};

/// How much of the run one sparkline column covers.
const BUCKET: Duration = Duration::from_secs(2);

/// How many columns of history to keep — 30 × 2 s, so the window *is* the last minute.
///
/// The averages are read over the whole window on purpose: over the run as a whole they
/// stop moving after a few minutes, and an ETA that no longer reacts to a network going
/// slow is worse than no ETA.
const BUCKETS: usize = 30;

const SPARK: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

/// Progress per bucket, newest last.
pub(crate) struct Rate {
  buckets: VecDeque<(usize, u64)>,
  /// Elapsed time at which the bucket being filled closes.
  bucket_end: Duration,
  last_done: usize,
  last_bytes: u64,
}

impl Rate {
  pub(crate) fn new() -> Self {
    Rate {
      buckets: VecDeque::with_capacity(BUCKETS),
      bucket_end: BUCKET,
      last_done: 0,
      last_bytes: 0,
    }
  }

  /// Feeds the window the run's running totals. Called once per frame; whether that is
  /// every 80 ms or every 3 s only decides how promptly a bucket closes.
  pub(crate) fn tick(&mut self, elapsed: Duration, done: usize, bytes: u64) {
    // A window's worth of catching up is as far as it is worth going: past that every
    // bucket would be an empty one, and the run was stalled anyway.
    if elapsed > self.bucket_end + BUCKET * BUCKETS as u32 {
      self.bucket_end = elapsed + BUCKET;
      self.buckets.clear();
      self.last_done = done;
      self.last_bytes = bytes;
      return;
    }

    while elapsed >= self.bucket_end {
      self
        .buckets
        .push_back((done - self.last_done, bytes - self.last_bytes));
      self.last_done = done;
      self.last_bytes = bytes;
      if self.buckets.len() > BUCKETS {
        self.buckets.pop_front();
      }
      self.bucket_end += BUCKET;
    }
  }

  /// The window as `width` sparkline cells, newest on the right.
  ///
  /// Scaled to the tallest bucket in view rather than to an absolute rate: what the line
  /// is for is seeing the shape change, and a fixed scale flattens every run that is not
  /// the fastest one.
  pub(crate) fn spark(&self, width: usize) -> String {
    let peak = self.buckets.iter().map(|&(n, _)| n).max().unwrap_or(0);
    let recent: Vec<usize> = self
      .buckets
      .iter()
      .rev()
      .take(width)
      .rev()
      .map(|&(n, _)| n)
      .collect();

    let mut out = " ".repeat(width.saturating_sub(recent.len()));
    for n in recent {
      out.push_str(match peak {
        0 => " ",
        _ => {
          let level = (n * (SPARK.len() - 1)).div_ceil(peak);
          SPARK[level.min(SPARK.len() - 1)]
        }
      });
    }
    out
  }

  /// Span of run time the window currently holds.
  fn span(&self) -> Duration {
    BUCKET * self.buckets.len() as u32
  }

  pub(crate) fn roms_per_min(&self) -> f64 {
    let secs = self.span().as_secs_f64();
    if secs == 0.0 {
      return 0.0;
    }
    let roms: usize = self.buckets.iter().map(|&(n, _)| n).sum();
    roms as f64 * 60.0 / secs
  }

  pub(crate) fn bytes_per_sec(&self) -> f64 {
    let secs = self.span().as_secs_f64();
    if secs == 0.0 {
      return 0.0;
    }
    let bytes: u64 = self.buckets.iter().map(|&(_, b)| b).sum();
    bytes as f64 / secs
  }

  /// How long `remaining` ROMs would take at the rate of the last minute.
  ///
  /// `None` while nothing has finished in the window: there is no honest number to show,
  /// and a made-up one is what people plan around.
  pub(crate) fn eta(&self, remaining: usize) -> Option<Duration> {
    if remaining == 0 {
      return Some(Duration::ZERO);
    }
    let per_min = self.roms_per_min();
    if per_min <= 0.0 {
      return None;
    }
    Some(Duration::from_secs_f64(remaining as f64 * 60.0 / per_min))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
  }

  /// One bucket closes per BUCKET of elapsed time, carrying what happened inside it.
  #[test]
  fn each_bucket_carries_the_progress_made_during_it() {
    let mut r = Rate::new();
    r.tick(secs(1), 3, 300); // still inside the first bucket
    assert_eq!(r.buckets.len(), 0);
    r.tick(secs(2), 4, 400);
    assert_eq!(r.buckets.back(), Some(&(4, 400)));
    r.tick(secs(4), 10, 900);
    assert_eq!(r.buckets.back(), Some(&(6, 500)));
  }

  /// The window is a minute long and never grows past it, however long the run is.
  #[test]
  fn the_window_never_holds_more_than_a_minute() {
    let mut r = Rate::new();
    for i in 1..=100u64 {
      r.tick(secs(i * 2), i as usize, 0);
    }
    assert_eq!(r.buckets.len(), BUCKETS);
    // Exactly one ROM per bucket for the last minute.
    assert_eq!(r.roms_per_min(), 30.0);
  }

  /// A frame that arrives long after the last one — the process was stopped, the
  /// terminal was suspended — must not walk the loop thousands of times to fill the
  /// window with empty buckets.
  #[test]
  fn a_long_gap_restarts_the_window_instead_of_filling_it() {
    let mut r = Rate::new();
    r.tick(secs(2), 5, 0);
    r.tick(secs(100_000), 6, 0);
    assert!(r.buckets.is_empty());
    // And it picks up from the totals it was handed, not from zero.
    r.tick(secs(100_002), 9, 0);
    assert_eq!(r.buckets.back(), Some(&(3, 0)));
  }

  /// Rates read over the window, not over the run.
  #[test]
  fn the_rates_are_read_over_the_window() {
    let mut r = Rate::new();
    r.tick(secs(2), 2, 2_000);
    r.tick(secs(4), 4, 4_000);
    // 4 ROMs and 4000 bytes over 4 s.
    assert_eq!(r.roms_per_min(), 60.0);
    assert_eq!(r.bytes_per_sec(), 1_000.0);
  }

  /// Nothing has finished yet, so there is no honest ETA to show.
  #[test]
  fn an_idle_window_has_no_eta() {
    let mut r = Rate::new();
    assert_eq!(r.eta(100), None);
    r.tick(secs(2), 0, 0);
    assert_eq!(r.eta(100), None);
    // Nothing left to do is a real answer, whatever the rate.
    assert_eq!(r.eta(0), Some(Duration::ZERO));
  }

  /// 60 ROMs a minute with 30 left is half a minute.
  #[test]
  fn the_eta_is_what_is_left_at_the_current_rate() {
    let mut r = Rate::new();
    r.tick(secs(2), 2, 0);
    assert_eq!(r.eta(30), Some(secs(30)));
  }

  /// The sparkline is exactly as wide as asked, newest on the right, and right-aligned
  /// while the window is still filling.
  #[test]
  fn the_sparkline_is_padded_left_until_the_window_fills() {
    let mut r = Rate::new();
    r.tick(secs(2), 1, 0);
    r.tick(secs(4), 5, 0);
    let s = r.spark(10);
    assert_eq!(s.chars().count(), 10);
    assert!(s.starts_with("        "));
    // Scaled to the tallest bucket in view: the newest one is the peak here.
    assert!(s.ends_with('█'));
  }

  /// A window with nothing in it draws blank rather than a floor of `▁`, which would
  /// read as a slow but steady run.
  #[test]
  fn an_empty_window_draws_blank() {
    let mut r = Rate::new();
    r.tick(secs(2), 0, 0);
    assert_eq!(r.spark(5), "     ");
  }
}
