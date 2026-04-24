use std::time::Duration;

/// End-of-run statistics printed after the TUI exits.
pub struct Summary {
  pub total: usize,
  pub success: usize,
  pub unchanged: usize,
  pub errors: usize,
  /// (kind, icon, roms_with_this_media) — canonical order from MEDIA_ICONS.
  pub media_stats: Vec<(&'static str, &'static str, usize)>,
  /// Average wall-clock time per step kind, in canonical pipeline order.
  /// Only populated for step kinds that actually ran at least once.
  pub step_avg_durations: Vec<(&'static str, Duration)>,
}

impl Summary {
  pub fn print(&self) {
    println!("\nrompom — {} ROMs\n", self.total);
    println!("  ✓  {:>4}  updated", self.success - self.unchanged);
    println!("  =  {:>4}  unchanged", self.unchanged);
    println!("  ✗  {:>4}  errors\n", self.errors);

    if self.success > 0 {
      println!("Media coverage");
      for &(kind, icon, found) in &self.media_stats {
        let bar = progress_bar(found, self.success, 20);
        let pct = found * 100 / self.success;
        println!(
          "  {}  {:<12}  {}  {}/{} ({}%)",
          icon, kind, bar, found, self.success, pct
        );
      }
      println!();
    }

    if !self.step_avg_durations.is_empty() {
      println!("Step timings (avg)");
      for &(label, dur) in &self.step_avg_durations {
        println!("  {:<20}  {:.2}s", label, dur.as_secs_f64());
      }
      println!();
    }
  }
}

fn progress_bar(done: usize, total: usize, width: usize) -> String {
  let filled = if total > 0 { done * width / total } else { 0 };
  let empty = width - filled;
  format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}
