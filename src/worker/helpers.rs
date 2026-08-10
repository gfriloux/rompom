use std::{collections::HashMap, path::Path};

use screenscraper::ApiFailure;

use crate::{
  package::{media_ext, Medias},
  rom::StepError,
};

pub(crate) const NAME_REGIONS: &[&str] = &["wor", "eu", "us", "fr", "jp", "ss"];

// ── ScreenScraper failures ─────────────────────────────────────────────────

/// What a failed ScreenScraper call means for the step.
///
/// `None` is the important one: it says ScreenScraper answered, and answered that it
/// does not know this ROM (404). That is the **only** case that justifies stopping the
/// run to ask the user. Everything else — a timeout, a saturated server, an exhausted
/// quota — is an error the ROM must not silently absorb, because absorbing it turns
/// "ScreenScraper is unreachable" into "identify these 400 games by hand".
pub(crate) fn lookup_failure(failure: ApiFailure) -> Option<StepError> {
  let reason = match failure {
    ApiFailure::NotFound => return None,
    ApiFailure::ThreadLimit => "ScreenScraper thread limit reached",
    ApiFailure::ServerBusy => "ScreenScraper is saturated or closed to inactive members",
    ApiFailure::Transport => "could not reach ScreenScraper",
    ApiFailure::QuotaExceeded => "daily ScreenScraper scrape quota exceeded",
    ApiFailure::KoQuotaExceeded => {
      "too many unrecognised ROMs today — ScreenScraper says come back tomorrow"
    }
    ApiFailure::ApiClosed => "the ScreenScraper API is closed",
    ApiFailure::Blacklisted => "this software version is blacklisted by ScreenScraper",
    ApiFailure::BadDevCredentials => "ScreenScraper rejected the developer credentials",
    ApiFailure::BadRequest => "ScreenScraper rejected the request as malformed",
    ApiFailure::Malformed => "ScreenScraper sent a response rompom could not parse",
    ApiFailure::Api => "ScreenScraper reported an error",
    ApiFailure::Http(_) => "unexpected ScreenScraper response",
  };
  // The status is worth carrying; the library error is not. `reqwest::Error` prints
  // ` for url (<full url>)` in its Display (error.rs:205), and `base_query()` puts
  // `devpassword` and `sspassword` in that query string — so interpolating the source
  // error here would write both passwords into the Completed panel, the end-of-run
  // summary and <system>.debug.log. rompom builds its own sentence instead.
  let reason = match failure {
    ApiFailure::Http(status) => format!("{} (HTTP {})", reason, status),
    _ => reason.to_string(),
  };
  Some(if failure.is_retryable() {
    StepError::Transient(reason)
  } else {
    StepError::Fatal(reason)
  })
}

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
  use screenscraper::jeuinfo::Media;
  use std::path::PathBuf;

  // ── lookup_failure ───────────────────────────────────────────────────────

  /// The whole point of P1.1: exactly one failure means "ask the user". Every other
  /// one used to land there too, so a saturated ScreenScraper produced a modal per ROM.
  #[test]
  fn only_a_404_sends_the_user_to_the_modal() {
    assert!(lookup_failure(ApiFailure::NotFound).is_none());

    for failure in [
      ApiFailure::BadRequest,
      ApiFailure::ServerBusy,
      ApiFailure::BadDevCredentials,
      ApiFailure::ApiClosed,
      ApiFailure::Blacklisted,
      ApiFailure::ThreadLimit,
      ApiFailure::QuotaExceeded,
      ApiFailure::KoQuotaExceeded,
      ApiFailure::Http(418),
      ApiFailure::Transport,
      ApiFailure::Malformed,
      ApiFailure::Api,
    ] {
      assert!(
        lookup_failure(failure).is_some(),
        "{:?} must not be mistaken for an unknown game",
        failure
      );
    }
  }

  /// Retrying is delegated to the library's own judgement, so the two stay in step.
  #[test]
  fn only_the_librarys_retryable_failures_are_transient() {
    for failure in [
      ApiFailure::ThreadLimit,
      ApiFailure::ServerBusy,
      ApiFailure::Transport,
    ] {
      assert!(
        matches!(lookup_failure(failure), Some(StepError::Transient(_))),
        "{:?} should be retried",
        failure
      );
    }

    for failure in [
      ApiFailure::QuotaExceeded,
      ApiFailure::KoQuotaExceeded,
      ApiFailure::ApiClosed,
      ApiFailure::Blacklisted,
      ApiFailure::BadDevCredentials,
      ApiFailure::BadRequest,
      ApiFailure::Malformed,
      ApiFailure::Api,
      ApiFailure::Http(500),
    ] {
      assert!(
        matches!(lookup_failure(failure), Some(StepError::Fatal(_))),
        "{:?} holds for the rest of the run and must not be retried",
        failure
      );
    }
  }

  /// An unexpected status is still worth naming precisely.
  #[test]
  fn an_unknown_status_carries_its_number() {
    let message = lookup_failure(ApiFailure::Http(503)).unwrap().to_string();
    assert!(message.contains("503"), "got: {}", message);
  }

  /// Every reason has to read as a sentence in the Completed panel, and none of them
  /// may quote the library error — `reqwest::Error` appends ` for url (<full url>)`,
  /// and that URL carries `devpassword` and `sspassword`.
  #[test]
  fn every_reason_is_a_plain_sentence_without_credentials() {
    for failure in [
      ApiFailure::BadRequest,
      ApiFailure::ServerBusy,
      ApiFailure::BadDevCredentials,
      ApiFailure::ApiClosed,
      ApiFailure::Blacklisted,
      ApiFailure::ThreadLimit,
      ApiFailure::QuotaExceeded,
      ApiFailure::KoQuotaExceeded,
      ApiFailure::Http(429),
      ApiFailure::Transport,
      ApiFailure::Malformed,
      ApiFailure::Api,
    ] {
      let message = lookup_failure(failure).unwrap().to_string();
      assert!(!message.is_empty());
      assert!(
        !message.contains("password") && !message.contains("url ("),
        "{:?} produced a message that could carry credentials: {}",
        failure,
        message
      );
    }
  }

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

  // ── search_name ──────────────────────────────────────────────────────────

  /// What ScreenScraper is asked for when the checksum lookup misses. Every tag left in
  /// spends a request and comes back empty, and a run that misses enough of them earns
  /// the 431 that closes the account for the day.
  #[test]
  fn search_name_strips_the_extension_and_the_tags() {
    assert_eq!(
      search_name("Sonic The Hedgehog (USA) [!].zip"),
      "Sonic The Hedgehog"
    );
    assert_eq!(search_name("Chrono Trigger (USA).sfc"), "Chrono Trigger");
    assert_eq!(search_name("Chrono Trigger [b1].sfc"), "Chrono Trigger");
    assert_eq!(
      search_name("Final Fantasy VII (USA) (Disc 1).chd"),
      "Final Fantasy VII"
    );
  }

  /// A clean name must come through untouched — the common case for a folder source
  /// someone has already tidied up.
  #[test]
  fn search_name_leaves_an_untagged_title_alone() {
    assert_eq!(search_name("Super Mario World.zip"), "Super Mario World");
    assert_eq!(search_name("Super Mario World"), "Super Mario World");
  }

  /// Degenerate, but reachable: a file named only by its tags. An empty search term is
  /// a request that cannot match, so what matters is that it does not panic on the way.
  #[test]
  fn search_name_survives_a_title_that_is_only_tags() {
    assert_eq!(search_name("(USA).zip"), "");
    assert_eq!(search_name("[!].zip"), "");
  }

  // ── check_media_changes ──────────────────────────────────────────────────

  fn media(sha1: &str) -> Media {
    Media {
      name: "box-2D".to_string(),
      parent: "jeu".to_string(),
      url: "https://screenscraper.fr/medias/1/2/box-2D.png".to_string(),
      region: Some("wor".to_string()),
      crc: String::new(),
      md5: String::new(),
      sha1: sha1.to_string(),
      size: None,
      format: "png".to_string(),
    }
  }

  fn state(entries: &[(&str, Option<&str>)]) -> HashMap<String, Option<String>> {
    entries
      .iter()
      .map(|(k, v)| (k.to_string(), v.map(|s| s.to_string())))
      .collect()
  }

  /// The steady state of any second run: nothing changed, so nothing is rebuilt and no
  /// pkgver moves. Getting this wrong bumps every package in the library for free.
  #[test]
  fn identical_medias_are_not_a_change() {
    let medias = Medias {
      image: Some(media("aaa")),
      video: Some(media("bbb")),
      ..Default::default()
    };

    let (changed, lines) = check_media_changes(
      &medias,
      &state(&[("image", Some("aaa")), ("video", Some("bbb"))]),
    );

    assert!(!changed);
    // One line per tracked kind, whether present or not — the --debug log reads as a
    // full inventory rather than a list of surprises.
    assert_eq!(lines.len(), 8);
  }

  /// ScreenScraper replacing an asset is the reason this function exists.
  #[test]
  fn a_new_sha1_on_a_media_is_a_change() {
    let medias = Medias {
      image: Some(media("aaa")),
      ..Default::default()
    };

    let (changed, lines) = check_media_changes(&medias, &state(&[("image", Some("bbb"))]));

    assert!(changed);
    assert!(
      lines
        .iter()
        .any(|l| l.contains("image") && l.contains("CHANGED")),
      "{:#?}",
      lines
    );
  }

  /// An asset ScreenScraper did not have last time and has now.
  #[test]
  fn a_media_that_appears_is_a_change() {
    let medias = Medias {
      marquee: Some(media("ccc")),
      ..Default::default()
    };

    let (changed, _) = check_media_changes(&medias, &state(&[]));

    assert!(changed);
  }

  /// And one it has withdrawn. The package must lose the file, so this counts too —
  /// an absent media compared against a recorded sha1 is not "unchanged".
  #[test]
  fn a_media_that_disappears_is_a_change() {
    let (changed, _) = check_media_changes(&Medias::default(), &state(&[("wheel", Some("ddd"))]));

    assert!(changed);
  }

  /// First run: no state at all, and ScreenScraper has nothing either. Nothing to
  /// download is not a change — otherwise every ROM without media would rebuild forever.
  #[test]
  fn nothing_on_either_side_is_not_a_change() {
    let (changed, _) = check_media_changes(&Medias::default(), &state(&[]));

    assert!(!changed);
  }
}
