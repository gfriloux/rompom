//! Turning a system's `source` block into the list of ROMs to process.
//!
//! Multi-disc grouping lives here rather than in `main.rs`: it is the one piece of the
//! collection stage that is pure, non-trivial and worth testing on its own.

use std::{
  collections::{BTreeMap, HashMap},
  path::Path,
};

use crate::rom::{DiscFile, RomSource, RomSourceData};

/// The disc number in one parenthesised group, `None` if it is not a disc indicator.
///
/// `inner` is the text between the parentheses — `"Disc 1"`, `"USA"`, `"CD 2"`.
fn disc_number(inner: &str) -> Option<u32> {
  let lower = inner.to_lowercase();
  let rest = ["disc", "disk", "cd"]
    .iter()
    .find_map(|prefix| lower.strip_prefix(prefix))?;
  let rest = rest.strip_prefix(' ').unwrap_or(rest);

  rest
    .chars()
    .take_while(|c| c.is_ascii_digit())
    .collect::<String>()
    .parse()
    .ok()
}

/// Detect `(Disc N)` / `(Disk N)` / `(CD N)` patterns in a filename stem.
///
/// Scans all parenthesised groups and returns `(base_name, disc_number)` for the last
/// matching group found, `None` if no disc indicator is present. The base name is the
/// stem **without that group** — the other groups all stay, wherever they sit.
///
/// Examples:
/// - `"Enemy Zero (USA) (Disc 0)"` → `Some(("Enemy Zero (USA)", 0))`
/// - `"Panzer Dragoon Saga (Disc 1)"` → `Some(("Panzer Dragoon Saga", 1))`
/// - `"Lunar (Disc 1) (Europe)"` → `Some(("Lunar (Europe)", 1))`
fn disc_indicator(stem: &str) -> Option<(String, u32)> {
  let mut result: Option<(String, u32)> = None;
  let mut search_from = 0;

  while let Some(rel) = stem[search_from..].find('(') {
    let open = search_from + rel;
    search_from = open + 1;

    let Some(close_rel) = stem[open + 1..].find(')') else {
      continue;
    };
    let close = open + 1 + close_rel;

    let Some(num) = disc_number(&stem[open + 1..close]) else {
      continue;
    };

    let mut base = stem[..open].trim_end().to_string();
    let tail = stem[close + 1..].trim();
    if !tail.is_empty() {
      if !base.is_empty() {
        base.push(' ');
      }
      base.push_str(tail);
    }
    result = Some((base, num));
  }

  result
}

/// Group multi-disc files into single `RomSourceData` entries.
///
/// Files whose stems match `(Disc N)` / `(Disk N)` / `(CD N)` and share the
/// same base name + extension are merged:
/// - Disc 1 becomes the primary entry (with the virtual `filename` = base + ext).
/// - Disc 2+ become `extra_discs` on that entry.
/// - Single-disc sources pass through unchanged.
pub(crate) fn group_multi_disc(sources: Vec<RomSourceData>) -> Vec<RomSourceData> {
  // ── Step 1: classify each source ───────────────────────────────────────
  struct Parsed {
    source: RomSourceData,
    base: String, // stem without disc indicator
    disc: Option<u32>,
    ext: String,
  }

  let parsed: Vec<Parsed> = sources
    .into_iter()
    .map(|src| {
      let filename = src.filename.clone();
      let stem = Path::new(&filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&filename)
        .to_string();
      let ext = Path::new(&filename)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
      match disc_indicator(&stem) {
        Some((base, num)) => Parsed {
          source: src,
          base,
          disc: Some(num),
          ext,
        },
        None => Parsed {
          source: src,
          base: stem,
          disc: None,
          ext,
        },
      }
    })
    .collect();

  // ── Step 2: find groups (base, ext) that have more than one disc entry ──
  // Use a BTreeMap so the final output order is deterministic.
  let mut group_count: HashMap<(String, String), usize> = HashMap::new();
  for p in &parsed {
    if p.disc.is_some() {
      *group_count
        .entry((p.base.clone(), p.ext.clone()))
        .or_insert(0usize) += 1;
    }
  }
  let multi: HashMap<(String, String), ()> = group_count
    .into_iter()
    .filter(|(_, c)| *c > 1)
    .map(|(k, _)| (k, ()))
    .collect();

  if multi.is_empty() {
    // Fast path: no multi-disc games, return sources as-is.
    return parsed.into_iter().map(|p| p.source).collect();
  }

  // ── Step 3: collect groups and pass-through entries ─────────────────────
  // groups: (base, ext) → sorted Vec<(disc_num, Parsed)>
  let mut groups: BTreeMap<(String, String), Vec<(u32, Parsed)>> = BTreeMap::new();
  let mut passthrough: Vec<RomSourceData> = Vec::new();

  for p in parsed {
    let key = (p.base.clone(), p.ext.clone());
    if let Some(disc_num) = p.disc.filter(|_| multi.contains_key(&key)) {
      groups.entry(key).or_default().push((disc_num, p));
    } else {
      passthrough.push(p.source);
    }
  }

  // ── Step 4: merge each group into one RomSourceData ─────────────────────
  let mut result: Vec<RomSourceData> = passthrough;

  for ((base, ext), mut discs) in groups {
    // Sort by disc number so disc 1 is always first.
    discs.sort_by_key(|(n, _)| *n);

    // Virtual logical filename: base name + extension (no disc indicator).
    let virtual_filename = if ext.is_empty() {
      base.clone()
    } else {
      format!("{}.{}", base, ext)
    };

    // Build extra_discs from disc 2+.
    let extra_discs: Vec<DiscFile> = discs[1..]
      .iter()
      .map(|(_, p)| {
        let src = &p.source;
        match &src.source {
          RomSource::InternetArchive(ia) => DiscFile {
            file_name: src.file_name.clone(),
            filename: src.filename.clone(),
            rom_url: ia.rom_url.clone(),
            sha1: ia.sha1.clone(),
            md5: ia.md5.clone(),
            crc32: ia.crc32.clone(),
            size: ia.size,
            local_path: None,
          },
          RomSource::Folder(f) => DiscFile {
            file_name: src.file_name.clone(),
            filename: src.filename.clone(),
            rom_url: String::new(),
            sha1: None,
            md5: None,
            crc32: None,
            size: 0,
            local_path: Some(f.local_path.clone()),
          },
        }
      })
      .collect();

    // Primary entry = disc 1, with the virtual filename.
    let (_, primary) = discs.remove(0);
    let mut primary_source = primary.source;
    primary_source.filename = virtual_filename;
    primary_source.extra_discs = extra_discs;
    result.push(primary_source);
  }

  result
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::rom::FolderSource;
  use std::path::PathBuf;

  /// A folder-source entry, which is all these two functions look at: they read
  /// `filename` and nothing else about where the file came from.
  fn source(filename: &str) -> RomSourceData {
    RomSourceData {
      file_name: format!("/roms/{}", filename),
      filename: filename.to_string(),
      source: RomSource::Folder(FolderSource {
        local_path: PathBuf::from(format!("/roms/{}", filename)),
      }),
      extra_discs: Vec::new(),
    }
  }

  fn names(sources: &[RomSourceData]) -> Vec<&str> {
    let mut out: Vec<&str> = sources.iter().map(|s| s.filename.as_str()).collect();
    out.sort();
    out
  }

  fn find<'a>(sources: &'a [RomSourceData], filename: &str) -> &'a RomSourceData {
    sources
      .iter()
      .find(|s| s.filename == filename)
      .unwrap_or_else(|| panic!("no entry named {:?} in {:?}", filename, names(sources)))
  }

  // ── disc_indicator ───────────────────────────────────────────────────────

  /// The overwhelming majority of a library. Anything that reads a disc number out of
  /// `Super Mario World (USA)` would merge unrelated games into one package.
  #[test]
  fn a_stem_without_a_disc_indicator_has_none() {
    assert_eq!(disc_indicator("Super Mario World"), None);
    assert_eq!(disc_indicator("Super Mario World (USA)"), None);
    assert_eq!(disc_indicator("Sonic The Hedgehog (USA) [!]"), None);
  }

  /// The three spellings seen in the wild, with and without the space, in any case.
  /// `(disk 2)` is not a typo to normalise away — No-Intro and Redump disagree.
  #[test]
  fn every_accepted_spelling_yields_the_same_base_and_number() {
    for stem in [
      "Final Fantasy VII (Disc 2)",
      "Final Fantasy VII (Disk 2)",
      "Final Fantasy VII (CD 2)",
      "Final Fantasy VII (disc2)",
      "Final Fantasy VII (DISC 2)",
    ] {
      assert_eq!(
        disc_indicator(stem),
        Some(("Final Fantasy VII".to_string(), 2)),
        "stem: {}",
        stem
      );
    }
  }

  /// Numbering starts at 0 in some sets and at 1 in others. Both have to survive, and
  /// disc 0 must not be read as "no disc".
  #[test]
  fn numbering_may_start_at_zero_or_one() {
    assert_eq!(
      disc_indicator("Enemy Zero (USA) (Disc 0)"),
      Some(("Enemy Zero (USA)".to_string(), 0))
    );
    assert_eq!(
      disc_indicator("Enemy Zero (USA) (Disc 1)"),
      Some(("Enemy Zero (USA)".to_string(), 1))
    );
  }

  /// A region tag before the indicator belongs to the base name: it is what tells two
  /// different releases of the same game apart, and dropping it merges them.
  #[test]
  fn a_region_tag_before_the_indicator_stays_in_the_base() {
    assert_eq!(
      disc_indicator("Panzer Dragoon Saga (USA) (Disc 1)"),
      Some(("Panzer Dragoon Saga (USA)".to_string(), 1))
    );
  }

  /// And so does one that comes after. The base used to be everything *before* the disc
  /// group, so a trailing `(USA)` was simply dropped.
  #[test]
  fn a_region_tag_after_the_indicator_stays_in_the_base_too() {
    assert_eq!(
      disc_indicator("Lunar (Disc 1) (USA)"),
      Some(("Lunar (USA)".to_string(), 1))
    );
    assert_eq!(
      disc_indicator("Lunar (Disc 2) (Europe)"),
      Some(("Lunar (Europe)".to_string(), 2))
    );
    assert_eq!(
      disc_indicator("Lunar (Rev 1) (Disc 1) (USA)"),
      Some(("Lunar (Rev 1) (USA)".to_string(), 1))
    );
  }

  /// The whole point of the base name: two regional releases must not merge. They used
  /// to share the base `"Lunar"` and came out as one package whose .m3u played disc 1 in
  /// English and disc 2 in French.
  #[test]
  fn two_regional_releases_do_not_merge_into_one_game() {
    let out = group_multi_disc(vec![
      source("Lunar (Disc 1) (USA).chd"),
      source("Lunar (Disc 2) (Europe).chd"),
    ]);

    assert_eq!(
      names(&out),
      ["Lunar (Disc 1) (USA).chd", "Lunar (Disc 2) (Europe).chd"]
    );
    assert!(out.iter().all(|s| s.extra_discs.is_empty()));
  }

  /// The same tag on both discs is the ordinary case and must still group — and the
  /// virtual name keeps the region, because that is what the package is called.
  #[test]
  fn the_same_tag_on_both_discs_still_groups() {
    let out = group_multi_disc(vec![
      source("Lunar (Disc 1) (USA).chd"),
      source("Lunar (Disc 2) (USA).chd"),
    ]);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].filename, "Lunar (USA).chd");
    assert_eq!(out[0].extra_discs.len(), 1);
  }

  // ── group_multi_disc ─────────────────────────────────────────────────────

  /// The fast path. A library with no multi-disc game must come out exactly as it went
  /// in — same entries, none of them carrying an extra disc.
  #[test]
  fn single_disc_sources_pass_through_untouched() {
    let out = group_multi_disc(vec![
      source("Super Mario World.zip"),
      source("Sonic The Hedgehog.zip"),
    ]);

    assert_eq!(
      names(&out),
      ["Sonic The Hedgehog.zip", "Super Mario World.zip"]
    );
    assert!(out.iter().all(|s| s.extra_discs.is_empty()));
  }

  /// The headline case: one package, disc 1 as the primary entry under a name with no
  /// disc indicator, disc 2 hanging off it.
  #[test]
  fn two_discs_become_one_entry_with_the_second_as_an_extra() {
    let out = group_multi_disc(vec![
      source("Panzer Dragoon Saga (Disc 1).chd"),
      source("Panzer Dragoon Saga (Disc 2).chd"),
    ]);

    assert_eq!(out.len(), 1);
    let game = &out[0];
    assert_eq!(game.filename, "Panzer Dragoon Saga.chd");
    // file_name still points at the real disc-1 file: that is what gets copied.
    assert_eq!(game.file_name, "/roms/Panzer Dragoon Saga (Disc 1).chd");
    assert_eq!(game.extra_discs.len(), 1);
    assert_eq!(
      game.extra_discs[0].filename,
      "Panzer Dragoon Saga (Disc 2).chd"
    );
  }

  /// Extra discs feed the .m3u playlist in order. Collection order is whatever the
  /// filesystem or the Internet Archive listing happened to give, so the sort matters.
  #[test]
  fn extra_discs_come_out_in_disc_order_whatever_the_input_order() {
    let out = group_multi_disc(vec![
      source("Final Fantasy VII (Disc 3).chd"),
      source("Final Fantasy VII (Disc 1).chd"),
      source("Final Fantasy VII (Disc 2).chd"),
    ]);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].file_name, "/roms/Final Fantasy VII (Disc 1).chd");
    let extras: Vec<&str> = out[0]
      .extra_discs
      .iter()
      .map(|d| d.filename.as_str())
      .collect();
    assert_eq!(
      extras,
      [
        "Final Fantasy VII (Disc 2).chd",
        "Final Fantasy VII (Disc 3).chd"
      ]
    );
  }

  /// Same title, two dump formats. Grouping across extensions would put a .chd and a
  /// .cue in one playlist and hand makepkg a package it cannot build.
  #[test]
  fn a_different_extension_is_a_different_game() {
    let out = group_multi_disc(vec![
      source("Lunar (Disc 1).chd"),
      source("Lunar (Disc 2).cue"),
    ]);

    assert_eq!(out.len(), 2);
    assert!(out.iter().all(|s| s.extra_discs.is_empty()));
  }

  /// A single file that happens to carry `(Disc 1)` is not a group. It keeps its own
  /// name — renaming it to the virtual one would break the state key and re-scrape it.
  #[test]
  fn a_lone_disc_one_is_not_a_group() {
    let out = group_multi_disc(vec![source("Riven (Disc 1).chd")]);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].filename, "Riven (Disc 1).chd");
    assert!(out[0].extra_discs.is_empty());
  }

  /// Grouped and ungrouped entries share the output. Nothing may be dropped on the way.
  #[test]
  fn ungrouped_entries_survive_alongside_a_group() {
    let out = group_multi_disc(vec![
      source("Sonic The Hedgehog.zip"),
      source("Lunar (Disc 1).chd"),
      source("Lunar (Disc 2).chd"),
      source("Super Mario World.zip"),
    ]);

    assert_eq!(
      names(&out),
      [
        "Lunar.chd",
        "Sonic The Hedgehog.zip",
        "Super Mario World.zip"
      ]
    );
    assert_eq!(find(&out, "Lunar.chd").extra_discs.len(), 1);
  }
}
