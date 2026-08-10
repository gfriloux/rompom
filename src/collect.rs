//! Turning a system's `source` block into the list of ROMs to process.
//!
//! Multi-disc grouping lives here rather than in `main.rs`: it is the one piece of the
//! collection stage that is pure, non-trivial and worth testing on its own.

use std::{
  collections::{BTreeMap, HashMap},
  path::Path,
};

use crate::rom::{DiscFile, RomSource, RomSourceData};

/// Detect `(Disc N)` / `(Disk N)` / `(CD N)` patterns in a filename stem.
///
/// Scans all parenthesised groups (e.g. a stem may also contain `(USA)` before
/// the disc indicator) and returns `(base_name, disc_number)` for the last
/// matching group found, `None` if no disc indicator is present.
///
/// Examples:
/// - `"Enemy Zero (USA) (Disc 0)"` → `Some(("Enemy Zero (USA)", 0))`
/// - `"Panzer Dragoon Saga (Disc 1)"` → `Some(("Panzer Dragoon Saga", 1))`
fn disc_indicator(stem: &str) -> Option<(String, u32)> {
  let mut result: Option<(String, u32)> = None;
  let mut search_from = 0;

  while let Some(rel) = stem[search_from..].find('(') {
    let paren = search_from + rel;
    let after = &stem[paren + 1..];
    let lower = after.to_lowercase();

    let num_offset = if lower.starts_with("disc ") || lower.starts_with("disk ") {
      5
    } else if lower.starts_with("disc") || lower.starts_with("disk") {
      4
    } else if lower.starts_with("cd ") {
      3
    } else if lower.starts_with("cd") {
      2
    } else {
      search_from = paren + 1;
      continue;
    };

    let digits: String = after[num_offset..]
      .chars()
      .take_while(|c| c.is_ascii_digit())
      .collect();

    if let Ok(num) = digits.parse::<u32>() {
      let base = stem[..paren].trim_end().to_string();
      result = Some((base, num));
    }

    search_from = paren + 1;
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
