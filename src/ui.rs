use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::time::Duration;

pub struct Ui {
  multi: MultiProgress,
}

pub struct RomProgress {
  bar: ProgressBar,
  filename: String,
}

pub struct PackagingProgress {
  bar: ProgressBar,
  label: String,
}

pub struct DownloadProgress {
  bar: ProgressBar,
  label: String,
}

// --- helpers ---

fn spinner_style() -> ProgressStyle {
  ProgressStyle::with_template("{spinner:.cyan} [{prefix}] {msg}")
    .unwrap()
    .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
}

fn done_style(symbol: &str) -> ProgressStyle {
  ProgressStyle::with_template(&format!("{} [{{prefix}}] {{msg}}", symbol)).unwrap()
}

fn make_bar(multi: &MultiProgress, index: usize, total: usize, msg: &str) -> ProgressBar {
  let bar = multi.add(ProgressBar::new_spinner());
  bar.set_style(spinner_style());
  bar.set_prefix(format!("{:>2}/{:>2}", index, total));
  bar.set_message(msg.to_string());
  bar.enable_steady_tick(Duration::from_millis(80));
  bar
}

fn finish_bar(bar: &ProgressBar, symbol: &str, msg: String) {
  bar.set_style(done_style(symbol));
  bar.finish_with_message(msg);
}

// --- Ui ---

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

  pub fn phase_discovery(&self, total: usize) {
    self
      .multi
      .println(format!("\nDiscovery — {} ROMs", total))
      .ok();
  }

  pub fn phase_packaging(&self, total: usize) {
    self
      .multi
      .println(format!("\nPackaging — {} ROMs", total))
      .ok();
  }

  pub fn phase_downloads(&self, total: usize) {
    self
      .multi
      .println(format!("\nDownloads — {} ROMs", total))
      .ok();
  }

  pub fn rom_discovery(&self, index: usize, total: usize, filename: &str) -> RomProgress {
    RomProgress {
      bar: make_bar(&self.multi, index, total, filename),
      filename: filename.to_string(),
    }
  }

  pub fn packaging_progress(&self, index: usize, total: usize, label: &str) -> PackagingProgress {
    PackagingProgress {
      bar: make_bar(&self.multi, index, total, label),
      label: label.to_string(),
    }
  }

  pub fn download_progress(&self, index: usize, total: usize, label: &str) -> DownloadProgress {
    DownloadProgress {
      bar: make_bar(&self.multi, index, total, label),
      label: label.to_string(),
    }
  }
}

// --- RomProgress (phase 1: discovery) ---

impl RomProgress {
  pub fn screenscraper_found(&self, name: &str) {
    finish_bar(&self.bar, "✓", format!("{} — {}", self.filename, name));
  }

  pub fn screenscraper_not_found(&self) {
    finish_bar(&self.bar, "⚠", format!("{} — (no match)", self.filename));
  }
}

// --- PackagingProgress (phase 2: packaging) ---

impl PackagingProgress {
  pub fn done(&self) {
    finish_bar(&self.bar, "✓", self.label.clone());
  }

  pub fn error(&self) {
    finish_bar(&self.bar, "✗", format!("{} — error", self.label));
  }
}

// --- DownloadProgress (phase 3: downloads) ---

#[allow(dead_code)]
impl DownloadProgress {
  pub fn rom_checking(&self) {
    self
      .bar
      .set_message(format!("{} — checking checksum...", self.label));
  }

  pub fn rom_downloading(&self) {
    self
      .bar
      .set_message(format!("{} — downloading...", self.label));
  }

  pub fn rom_redownloading(&self) {
    self.bar.set_message(format!(
      "{} — checksum mismatch, re-downloading...",
      self.label
    ));
  }

  pub fn rom_done(&self) {
    self.bar.set_message(format!("{} — ROM ✓", self.label));
  }

  pub fn rom_skipped(&self) {
    self
      .bar
      .set_message(format!("{} — ROM ✓ (already exists)", self.label));
  }

  pub fn start_media(&self, kind: &str) {
    self
      .bar
      .set_message(format!("{} — {} — downloading...", self.label, kind));
  }

  pub fn media_done(&self, kind: &str) {
    self.bar.set_message(format!("{} — {} ✓", self.label, kind));
  }

  pub fn media_unavailable(&self, kind: &str) {
    self
      .bar
      .set_message(format!("{} — {} — not available", self.label, kind));
  }

  pub fn finish(&self) {
    finish_bar(&self.bar, "✓", self.label.clone());
  }

  pub fn finish_error(&self) {
    finish_bar(&self.bar, "✗", format!("{} — error", self.label));
  }
}
