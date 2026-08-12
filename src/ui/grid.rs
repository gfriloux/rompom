//! Layout arithmetic for the ROM grid.
//!
//! Nothing here takes a `Frame` or touches a terminal: column widths, the visible
//! window and text fitting are decided from numbers alone. That is what makes a
//! rendering change testable at all — the interface itself cannot be opened from a
//! process with no controlling terminal.

use std::time::Duration;

use super::{errors, Cell, Dot, RomEntry, MEDIA_COUNT};

#[cfg(test)]
use super::RomInfo;

/// Cells taken by one media dot, gap included.
const MEDIA_CELL: u16 = 3;

/// Below this width the full layout does not fit — 89 cells of fixed columns leave
/// nothing for the status — so the grid folds.
const FOLD_WIDTH: u16 = 100;

/// Column widths for one grid row, in terminal cells.
///
/// Every width but `status` comes from the design spec (`design/tui/handoff.md`) and
/// carries two cells of margin over its longest content, so neighbouring columns never
/// touch. `status` takes whatever is left.
pub(crate) struct Columns {
  pub(crate) index: u16,
  pub(crate) name: u16,
  pub(crate) id: u16,
  pub(crate) pkg: u16,
  pub(crate) rom: u16,
  pub(crate) media_cell: u16,
  pub(crate) time: u16,
  pub(crate) status: u16,
  /// Whether this is the folded layout, which the caller needs to know for the things
  /// widths cannot express: the shortened status wording and the compact banner.
  pub(crate) folded: bool,
}

/// The grid layout for a terminal this wide.
///
/// Two layouts, not one that shrinks continuously: the arrival index and the clock are
/// dropped outright below `FOLD_WIDTH` rather than squeezed. Both are worth their cells
/// when there is room and neither is worth taking cells from the ROM's name.
pub(crate) fn columns(width: u16) -> Columns {
  let folded = width < FOLD_WIDTH;
  let c = if folded {
    Columns {
      index: 0,
      name: 26,
      id: 4,
      pkg: 4,
      rom: 5,
      media_cell: 2,
      time: 0,
      status: 0,
      folded,
    }
  } else {
    Columns {
      index: 6,
      name: 30,
      id: 5,
      pkg: 5,
      rom: 6,
      media_cell: MEDIA_CELL,
      time: 10,
      status: 0,
      folded,
    }
  };
  let fixed = c.index + c.name + c.id + c.pkg + c.rom + c.media_cell * MEDIA_COUNT as u16 + c.time;
  Columns {
    status: width.saturating_sub(fixed),
    ..c
  }
}

/// The status of a row in as few cells as the folded layout can spare.
///
/// Derived from the cells rather than cut down from the status phrase: `"checksum
/// mismatch, re-downloading"` truncated to five cells says `"chec…"`, which is both
/// unreadable and indistinguishable from a checksum *failure*.
pub(crate) fn short_status(entry: &RomEntry) -> String {
  if let Some(cause) = &entry.error {
    return errors::classify(cause).label().to_string();
  }
  if entry.finished() {
    return if entry.unchanged { "same" } else { "ok" }.to_string();
  }
  if entry.id == Cell::Waiting {
    return "id · m".to_string();
  }
  if entry.media.contains(&Dot::Running) {
    let done = entry.media.iter().filter(|d| **d != Dot::Todo).count();
    return format!("{}/{}", done, MEDIA_COUNT);
  }
  if matches!(entry.rom, Cell::Running | Cell::Progress(_)) {
    return "rom".to_string();
  }
  if entry.pkg == Cell::Running {
    return "pkg".to_string();
  }
  if entry.id == Cell::Running {
    return "scrap".to_string();
  }
  if entry.started_at.is_none() {
    return "queued".to_string();
  }
  "wait".to_string()
}

/// Column widths for the errors view, which trades the media dots and the clock for the
/// two things a failed ROM is read for.
pub(crate) struct ErrorColumns {
  pub(crate) index: u16,
  pub(crate) name: u16,
  pub(crate) id: u16,
  pub(crate) pkg: u16,
  pub(crate) rom: u16,
  pub(crate) cause: u16,
  pub(crate) attempts: u16,
}

pub(crate) fn error_columns(width: u16) -> ErrorColumns {
  let fixed = 6 + 30 + 5 + 5 + 6 + 26;
  ErrorColumns {
    index: 6,
    name: 30,
    id: 5,
    pkg: 5,
    rom: 6,
    cause: 26,
    attempts: width.saturating_sub(fixed),
  }
}

/// Fits `text` into exactly `width` cells: padded with spaces, or cut and marked as cut.
///
/// Grid columns are positional — a value one cell too long shifts every column after it
/// for that row only, which reads as corruption rather than as a long name.
pub(crate) fn fit(text: &str, width: usize) -> String {
  let cut = truncate(text, width);
  let padding = width.saturating_sub(cut.chars().count());
  format!("{}{}", cut, " ".repeat(padding))
}

/// Cuts `text` down to `width` cells, appending `…` when something was dropped.
///
/// Newlines are flattened first: a download error may well carry one, and a line break
/// inside a row would push every following row down by one and desynchronise the grid
/// from its scroll window.
pub(crate) fn truncate(text: &str, width: usize) -> String {
  let text = text.replace('\n', " ");
  match width {
    0 => String::new(),
    1 => "…".to_string(),
    _ if text.chars().count() <= width => text,
    _ => {
      let kept: String = text.chars().take(width - 1).collect();
      format!("{}…", kept.trim_end())
    }
  }
}

/// The row the visible window must keep on screen.
///
/// The newest ROM that has started and not finished — that is where the workers are.
/// With nothing in flight, the first ROM still queued, so the window sits on what is
/// about to happen rather than on the top of a list nobody is reading any more.
pub(crate) fn active_anchor(roms: &[&RomEntry]) -> usize {
  if let Some(i) = roms
    .iter()
    .rposition(|r| r.started_at.is_some() && r.finished_at.is_none())
  {
    return i;
  }
  roms
    .iter()
    .position(|r| r.started_at.is_none())
    .unwrap_or_else(|| roms.len().saturating_sub(1))
}

/// First visible row, so that `anchor` sits a third of the way down the window.
///
/// A third rather than the middle: what is below the anchor is the queue, and it is
/// worth more screen than the ROMs already finished above it.
pub(crate) fn scroll_offset(len: usize, height: usize, anchor: usize) -> usize {
  if len <= height || height == 0 {
    return 0;
  }
  let lead = height / 3;
  anchor.saturating_sub(lead).min(len - height)
}

/// Keeps `selected` inside the window, moving it as little as possible.
///
/// Recentring on every keypress the way the follow mode does would make the whole list
/// slide under a cursor the user is trying to aim with. The window only moves when the
/// selection is about to leave it.
pub(crate) fn clamp_scroll(scroll: usize, len: usize, height: usize, selected: usize) -> usize {
  if len <= height || height == 0 {
    return 0;
  }
  let max = len - height;
  // Clamped first: the list shrinks between frames — a resize, a shorter filter — and a
  // remembered offset past the end would otherwise scroll into empty space.
  let scroll = scroll.min(max);
  if selected < scroll {
    selected
  } else if selected >= scroll + height {
    (selected + 1 - height).min(max)
  } else {
    scroll
  }
}

/// `1.5 MiB`, `912 KiB`, `29.2 GiB` — the unit a ROM collection is actually discussed in.
pub(crate) fn format_bytes(bytes: u64) -> String {
  const KIB: f64 = 1024.0;
  let b = bytes as f64;
  if b < KIB {
    format!("{} B", bytes)
  } else if b < KIB * KIB {
    format!("{:.0} KiB", b / KIB)
  } else if b < KIB * KIB * KIB {
    format!("{:.1} MiB", b / (KIB * KIB))
  } else {
    format!("{:.1} GiB", b / (KIB * KIB * KIB))
  }
}

/// `4.6s`, `42s`, `3m 10s`, `1h 04m` — one shape per order of magnitude.
///
/// Tenths below ten seconds only: past that they are noise, and the column is 10 cells.
pub(crate) fn format_elapsed(d: Duration) -> String {
  let secs = d.as_secs_f64();
  if secs < 10.0 {
    format!("{:.1}s", secs)
  } else if secs < 60.0 {
    format!("{}s", secs as u64)
  } else if secs < 3600.0 {
    format!("{}m {:02}s", secs as u64 / 60, secs as u64 % 60)
  } else {
    format!("{}h {:02}m", secs as u64 / 3600, (secs as u64 % 3600) / 60)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::time::Instant;

  fn refs(roms: &[RomEntry]) -> Vec<&RomEntry> {
    roms.iter().collect()
  }

  fn entry(started: bool, finished: bool) -> RomEntry {
    let mut e = RomEntry::queued(RomInfo {
      label: "rom.zip".to_string(),
      file_name: "rom.zip".to_string(),
      size: None,
      sha1: None,
      source: String::new(),
    });
    if started {
      e.started_at = Some(Instant::now());
    }
    if finished {
      e.finished_at = Some(Instant::now());
    }
    e
  }

  /// The eight fixed columns are the spec's, and `status` is whatever a wide terminal
  /// has left over.
  #[test]
  fn the_fixed_columns_come_from_the_spec() {
    let c = columns(120);
    assert_eq!(
      (c.index, c.name, c.id, c.pkg, c.rom, c.media_cell, c.time),
      (6, 30, 5, 5, 6, 3, 10)
    );
    // 6 + 30 + 5 + 5 + 6 + 9×3 + 10 = 89
    assert_eq!(c.status, 120 - 89);
  }

  /// Below 100 cells the index and the clock go entirely, rather than every column
  /// giving up a cell — and the status still gets room, which is the whole point.
  #[test]
  fn a_narrow_terminal_folds_instead_of_squeezing() {
    let c = columns(80);
    assert!(c.folded);
    assert_eq!((c.index, c.time), (0, 0));
    assert_eq!((c.name, c.id, c.pkg, c.rom, c.media_cell), (26, 4, 4, 5, 2));
    // 26 + 4 + 4 + 5 + 9×2 = 57
    assert_eq!(c.status, 80 - 57);
  }

  /// The threshold is where the full layout stops fitting, not one cell either side.
  #[test]
  fn the_fold_happens_at_a_hundred_columns() {
    assert!(columns(99).folded);
    assert!(!columns(100).folded);
  }

  /// A terminal narrower than the folded columns themselves must not underflow.
  #[test]
  fn an_absurdly_narrow_terminal_does_not_underflow() {
    assert_eq!(columns(20).status, 0);
    assert_eq!(columns(0).status, 0);
  }

  /// The short status comes from the cells, not from cutting the phrase down: five
  /// cells of "checksum mismatch, re-downloading" reads as a checksum failure.
  #[test]
  fn the_short_status_says_what_stage_the_row_is_on() {
    let mut e = entry(true, false);
    e.id = Cell::Running;
    assert_eq!(short_status(&e), "scrap");
    e.id = Cell::Done;
    e.pkg = Cell::Running;
    assert_eq!(short_status(&e), "pkg");
    e.pkg = Cell::Done;
    e.rom = Cell::Running;
    assert_eq!(short_status(&e), "rom");
    e.id = Cell::Waiting;
    assert_eq!(short_status(&e), "id · m");
  }

  /// Media in flight reads as how far through the nine it is.
  #[test]
  fn media_in_flight_shows_how_far_through_it_is() {
    let mut e = entry(true, false);
    e.media[0] = Dot::Fresh;
    e.media[1] = Dot::Unchanged;
    e.media[2] = Dot::Running;
    assert_eq!(short_status(&e), "3/9");
  }

  /// A finished row says how it finished, and a failed one says what kind of failure.
  #[test]
  fn a_finished_row_says_how_it_ended() {
    let mut e = entry(true, true);
    assert_eq!(short_status(&e), "ok");
    e.unchanged = true;
    assert_eq!(short_status(&e), "same");
    e.error = Some("Checksum mismatch: expected a, got b".to_string());
    assert_eq!(short_status(&e), "checksum");
    assert_eq!(short_status(&entry(false, false)), "queued");
  }

  /// A value shorter than its column is padded, so the next column starts where the
  /// header says it does.
  #[test]
  fn a_short_value_is_padded_to_its_column() {
    assert_eq!(fit("ok", 6), "ok    ");
    assert_eq!(fit("", 3), "   ");
  }

  /// A value longer than its column is cut to exactly the column width — one cell over
  /// would shift every column after it on that row alone.
  #[test]
  fn a_long_value_is_cut_to_exactly_its_column() {
    let cut = fit("Zombies Ate My Neighbors and Then Some", 20);
    assert_eq!(cut.chars().count(), 20);
    assert!(cut.ends_with('…'));
  }

  /// A cause that fits is shown as is — no gratuitous ellipsis.
  #[test]
  fn a_short_cause_is_left_alone() {
    assert_eq!(truncate("host unreachable", 40), "host unreachable");
    assert_eq!(truncate("exact", 5), "exact");
  }

  /// Past the column width the line would run over the border and push the ROM name out
  /// of view, so the cause is cut and marked as cut.
  #[test]
  fn a_long_cause_is_cut_and_says_so() {
    let cause = "too many unrecognised ROMs today — ScreenScraper says come back tomorrow";
    let cut = truncate(cause, 20);
    assert_eq!(cut.chars().count(), 20);
    assert!(cut.ends_with('…'));
    assert!(cause.starts_with(cut.trim_end_matches('…')));
  }

  /// A narrow terminal must not panic on the arithmetic, and must not emit a bare
  /// dangling ellipsis wider than the space it was given.
  #[test]
  fn a_narrow_column_does_not_overflow() {
    assert_eq!(truncate("anything", 0), "");
    assert_eq!(truncate("anything", 1), "…");
    assert_eq!(truncate("anything", 2).chars().count(), 2);
  }

  /// When the cut lands just after a space, keeping it renders as a gap floating before
  /// the ellipsis. Cutting mid-word is left alone — the ellipsis says enough.
  #[test]
  fn the_cut_does_not_leave_a_dangling_space() {
    assert_eq!(truncate("could not reach it", 11), "could not…");
    assert_eq!(truncate("could not reach it", 12), "could not r…");
  }

  /// Causes reach the grid from `StepStatus::Failed`, and a panic message carries the
  /// panic location, which contains no newline — but a download error may well. A row
  /// that spans two lines desynchronises the grid from its scroll window.
  #[test]
  fn newlines_would_break_the_row_layout() {
    assert_eq!(truncate("first\nsecond", 40), "first second");
    assert_eq!(fit("first\nsecond", 12), "first second");
  }

  /// The workers are at the newest in-flight ROM, so that is what the window follows.
  #[test]
  fn the_anchor_is_the_newest_rom_in_flight() {
    let roms = vec![
      entry(true, true),
      entry(true, false),
      entry(true, false),
      entry(false, false),
    ];
    assert_eq!(active_anchor(&refs(&roms)), 2);
  }

  /// Between two batches nothing is in flight; the window then sits on what is about to
  /// start rather than on the finished ROMs above it.
  #[test]
  fn with_nothing_in_flight_the_anchor_is_the_first_queued_rom() {
    let roms = vec![entry(true, true), entry(true, true), entry(false, false)];
    assert_eq!(active_anchor(&refs(&roms)), 2);
  }

  /// End of run: everything is finished, and the anchor must still be a valid index.
  #[test]
  fn a_finished_run_anchors_on_the_last_row() {
    let roms = vec![entry(true, true), entry(true, true)];
    assert_eq!(active_anchor(&refs(&roms)), 1);
    assert_eq!(active_anchor(&[]), 0);
  }

  /// A list that fits on screen never scrolls, whatever the anchor says.
  #[test]
  fn a_list_that_fits_never_scrolls() {
    assert_eq!(scroll_offset(5, 20, 4), 0);
    assert_eq!(scroll_offset(20, 20, 19), 0);
  }

  /// The anchor sits a third of the way down, and the window never runs past the end of
  /// the list — the rows below the anchor are the queue, and they are worth showing.
  #[test]
  fn the_window_puts_the_anchor_a_third_of_the_way_down() {
    assert_eq!(scroll_offset(100, 30, 50), 40);
    // Near the top there is nothing to scroll past.
    assert_eq!(scroll_offset(100, 30, 5), 0);
    // Near the end the window stops at the last full page.
    assert_eq!(scroll_offset(100, 30, 99), 70);
  }

  /// A zero-height window is what a terminal one line tall gives us.
  #[test]
  fn a_zero_height_window_does_not_divide_by_zero() {
    assert_eq!(scroll_offset(100, 0, 50), 0);
  }

  /// A selection inside the window leaves it exactly where it was: aiming with the
  /// cursor must not make the whole list slide.
  #[test]
  fn a_visible_selection_does_not_move_the_window() {
    assert_eq!(clamp_scroll(40, 100, 30, 50), 40);
    assert_eq!(clamp_scroll(40, 100, 30, 40), 40);
    assert_eq!(clamp_scroll(40, 100, 30, 69), 40);
  }

  /// Past either edge the window follows by the smallest step that brings the selection
  /// back — one row, not a recentring.
  #[test]
  fn the_window_follows_a_selection_that_leaves_it() {
    assert_eq!(clamp_scroll(40, 100, 30, 39), 39);
    assert_eq!(clamp_scroll(40, 100, 30, 70), 41);
  }

  /// A window taller than the list never scrolls, and an offset remembered from a
  /// longer list — a resize, a filter that shrank — still lands on the selection
  /// instead of scrolling into empty space.
  #[test]
  fn the_clamped_window_stays_within_the_list() {
    assert_eq!(clamp_scroll(12, 5, 20, 3), 0);
    assert_eq!(clamp_scroll(999, 100, 30, 50), 50);
    assert_eq!(clamp_scroll(999, 100, 30, 95), 70);
    assert_eq!(clamp_scroll(5, 100, 0, 3), 0);
  }

  /// The unit changes with the magnitude, and a ROM-sized file reads in MiB.
  #[test]
  fn byte_sizes_read_in_the_unit_of_their_magnitude() {
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(1024), "1 KiB");
    assert_eq!(format_bytes(1_572_864), "1.5 MiB");
    assert_eq!(format_bytes(31_353_665_945), "29.2 GiB");
  }

  /// One shape per order of magnitude, and the widest still fits the 10-cell column.
  #[test]
  fn elapsed_time_changes_shape_with_its_magnitude() {
    assert_eq!(format_elapsed(Duration::from_millis(4600)), "4.6s");
    assert_eq!(format_elapsed(Duration::from_secs(42)), "42s");
    assert_eq!(format_elapsed(Duration::from_secs(190)), "3m 10s");
    assert_eq!(format_elapsed(Duration::from_secs(2472)), "41m 12s");
    assert_eq!(format_elapsed(Duration::from_secs(3840)), "1h 04m");
    assert!(format_elapsed(Duration::from_secs(359_999)).chars().count() <= 10);
  }
}
