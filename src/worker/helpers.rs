use std::{collections::HashMap, path::Path};

use crate::package::Medias;

pub(crate) const NAME_REGIONS: &[&str] = &["wor", "eu", "us", "fr", "jp", "ss"];

/// Strips the file extension and region/revision tags from a ROM filename to
/// produce a clean title suitable for a ScreenScraper name search.
///
/// `"Sonic The Hedgehog (USA) [!].zip"` → `"Sonic The Hedgehog"`
pub(crate) fn search_name(filename: &str) -> String {
  let stem = Path::new(filename)
    .file_stem()
    .and_then(|s| s.to_str())
    .unwrap_or(filename);
  stem
    .split('(')
    .next()
    .and_then(|s| s.split('[').next())
    .unwrap_or(stem)
    .trim()
    .to_string()
}

/// Returns the output filename for a downloaded media asset.
pub(crate) fn media_filename(kind: &str, format: &str) -> String {
  match kind {
    "video" => "video.mp4".to_string(),
    "manual" => "manual.pdf".to_string(),
    _ => format!("{}.{}", kind, format),
  }
}

/// Compares current media sha1s (from SS) against the saved state.
///
/// Returns `(changed, log_lines)` where `changed` is true if at least one
/// media sha1 differs, and `log_lines` has one entry per media type with
/// the comparison result (for `--debug` output).
pub(crate) fn check_media_changes(
  medias: &Medias,
  prev: &HashMap<String, Option<String>>,
) -> (bool, Vec<String>) {
  let mut changed = false;
  let mut lines = Vec::new();

  for (kind, media) in [
    ("video", medias.video.as_ref()),
    ("image", medias.image.as_ref()),
    ("thumbnail", medias.thumbnail.as_ref()),
    ("bezel", medias.bezel.as_ref()),
    ("marquee", medias.marquee.as_ref()),
    ("screenshot", medias.screenshot.as_ref()),
    ("wheel", medias.wheel.as_ref()),
    ("manual", medias.manual.as_ref()),
  ] {
    let new_sha1 = media.map(|m| m.sha1.as_str());
    let prev_sha1 = prev.get(kind).and_then(|v| v.as_deref());
    if new_sha1 != prev_sha1 {
      changed = true;
      lines.push(format!(
        "[BuildPackage] media {:<12}: CHANGED  state={}  ss={}",
        kind,
        prev_sha1.unwrap_or("(absent)"),
        new_sha1.unwrap_or("(absent)")
      ));
    } else {
      lines.push(format!(
        "[BuildPackage] media {:<12}: ok       ({})",
        kind,
        new_sha1.unwrap_or("absent")
      ));
    }
  }

  (changed, lines)
}
