/// End-of-run statistics printed after the TUI exits.
pub struct Summary {
  pub total: usize,
  pub success: usize,
  pub errors: usize,
  /// (kind, icon, roms_with_this_media) — canonical order from MEDIA_ICONS.
  pub media_stats: Vec<(&'static str, &'static str, usize)>,
}

impl Summary {
  pub fn print(&self) {
    println!("\nrompom — {} ROMs\n", self.total);
    println!("  ✓  {:>4}  completed", self.success);
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
    }
    println!();
  }
}

fn progress_bar(done: usize, total: usize, width: usize) -> String {
  let filled = if total > 0 { done * width / total } else { 0 };
  let empty = width - filled;
  format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}
