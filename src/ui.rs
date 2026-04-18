use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::time::Duration;

pub struct Ui {
  multi: MultiProgress,
}

pub struct RomBar {
  bar: ProgressBar,
  label: String,
}

fn spinner_style() -> ProgressStyle {
  ProgressStyle::with_template("{spinner:.cyan} [{prefix}] {msg}")
    .unwrap()
    .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
}

fn done_style(symbol: &str) -> ProgressStyle {
  ProgressStyle::with_template(&format!("{} [{{prefix}}] {{msg}}", symbol)).unwrap()
}

impl Ui {
  pub fn new() -> Self {
    Ui {
      multi: MultiProgress::new(),
    }
  }

  pub fn fetching_metadata(&self, item: &str) {
    self
      .multi
      .println(format!("Fetching metadata: {}", item))
      .ok();
  }

  pub fn new_rom_bar(&self, index: usize, total: usize, filename: &str) -> RomBar {
    let bar = self.multi.add(ProgressBar::new_spinner());
    bar.set_style(spinner_style());
    bar.set_prefix(format!("{:>3}/{:>3}", index, total));
    bar.set_message(filename.to_string());
    bar.enable_steady_tick(Duration::from_millis(80));
    RomBar {
      bar,
      label: filename.to_string(),
    }
  }
}

impl RomBar {
  fn msg(&self, text: &str) {
    self.bar.set_message(text.to_string());
  }

  // Phase 1 — Discovery
  pub fn discovering(&self) {
    self.msg(&format!("{} — discovering...", self.label));
  }

  pub fn found(&mut self, name: &str) {
    self.label = name.to_string();
    self.msg(&format!("{} — packaging...", self.label));
  }

  pub fn not_found(&self) {
    self.msg(&format!("{} — (not found) packaging...", self.label));
  }

  // Phase 2 — Packaging
  pub fn packaging(&self) {
    self.msg(&format!("{} — packaging...", self.label));
  }

  // Phase 3 — ROM
  pub fn rom_checking(&self) {
    self.msg(&format!("{} — checking...", self.label));
  }

  pub fn rom_downloading(&self) {
    self.msg(&format!("{} — downloading...", self.label));
  }

  pub fn rom_redownloading(&self) {
    self.msg(&format!(
      "{} — checksum mismatch, re-downloading...",
      self.label
    ));
  }

  pub fn rom_done(&self) {
    self.msg(&format!("{} — ROM ✓", self.label));
  }

  pub fn rom_skipped(&self) {
    self.msg(&format!("{} — ROM ✓ (already exists)", self.label));
  }

  // Phase 3 — Media
  pub fn start_media(&self, kind: &str) {
    self.msg(&format!("{} — {} — downloading...", self.label, kind));
  }

  pub fn media_done(&self, kind: &str) {
    self.msg(&format!("{} — {} ✓", self.label, kind));
  }

  pub fn media_unavailable(&self, kind: &str) {
    self.msg(&format!("{} — {} — not available", self.label, kind));
  }

  // Fin
  pub fn finish(&self) {
    self.bar.set_style(done_style("✓"));
    self.bar.finish_with_message(self.label.clone());
  }

  pub fn finish_error(&self) {
    self.bar.set_style(done_style("✗"));
    self
      .bar
      .finish_with_message(format!("{} — error", self.label));
  }
}
