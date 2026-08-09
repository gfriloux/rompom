use std::{collections::HashMap, path::Path};

use crate::package::{media_ext, Medias};

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
///
/// The result is joined onto the ROM's output directory, and `Path::join` happily
/// walks out of it: a ScreenScraper format of `png/../../x` would have written
/// outside the tree. `media_ext` whitelists the extension, so the value returned here
/// is always a single path component.
pub(crate) fn media_filename(kind: &str, format: &str) -> String {
  match kind {
    "video" => "video.mp4".to_string(),
    "manual" => "manual.pdf".to_string(),
    _ => format!("{}.{}", kind, media_ext(format)),
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

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::PathBuf;

  /// The destination is built as `directory.join(media_filename(...))`, and Path::join
  /// resolves `..` against the directory rather than rejecting it. Before the fix,
  /// a format of `png/../../x` produced `image.png/../../x`, which lands two levels
  /// above the ROM's output directory.
  #[test]
  fn media_filename_stays_inside_the_output_directory() {
    let directory = PathBuf::from("/out/roms/sonic");

    for hostile in [
      "png/../../x",
      "../../etc/passwd",
      "png/../..",
      "/etc/passwd",
      "png\\..\\..",
    ] {
      let name = media_filename("image", hostile);
      assert!(
        !name.contains('/') && !name.contains('\\') && !name.contains(".."),
        "format {hostile:?} produced {name:?}"
      );

      // One path component, and the join cannot leave the directory.
      let dest = directory.join(&name);
      assert_eq!(dest.parent(), Some(directory.as_path()));
      assert!(dest.starts_with(&directory));
    }
  }

  /// The fixed kinds keep their own extension, whatever ScreenScraper claims.
  #[test]
  fn media_filename_keeps_the_canonical_names() {
    assert_eq!(media_filename("video", "../../x"), "video.mp4");
    assert_eq!(media_filename("manual", "../../x"), "manual.pdf");
    assert_eq!(media_filename("image", "png"), "image.png");
    assert_eq!(media_filename("thumbnail", "jpg"), "thumbnail.jpg");
  }

  /// A format that whitelists down to nothing must still yield a usable name, and the
  /// same one the PKGBUILD source entry uses.
  #[test]
  fn media_filename_falls_back_when_the_format_is_unusable() {
    assert_eq!(media_filename("image", "../.."), "image.bin");
    assert_eq!(media_filename("image", ""), "image.bin");
  }
}
