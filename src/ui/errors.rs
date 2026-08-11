//! Classifying and counting failure causes for the errors view.
//!
//! Fifteen failures scrolling past as fifteen distinct sentences say nothing; the same
//! fifteen as `9 checksum · 4 screenscraper · 2 download` say whether the run hit a bad
//! mirror, an exhausted quota, or a flaky link.

/// What kind of failure a cause describes.
///
/// Matched on the cause text, because that is all that survives: the step is gone from
/// memory by the time the row is drawn, and `StepStatus::Failed` carries a `String`.
/// Ordering in `classify` matters — a checksum mismatch on a download is a checksum
/// problem, not a download one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ErrorKind {
  /// The bytes arrived but are not the bytes that were asked for.
  Checksum,
  /// ScreenScraper refused, was unreachable, or the ROM was never identified.
  ScreenScraper,
  /// The transfer itself failed: no server answered, the disk did not cooperate.
  Download,
  Other,
}

impl ErrorKind {
  pub(crate) fn label(self) -> &'static str {
    match self {
      ErrorKind::Checksum => "checksum",
      ErrorKind::ScreenScraper => "screenscraper",
      ErrorKind::Download => "download",
      ErrorKind::Other => "other",
    }
  }
}

pub(crate) fn classify(cause: &str) -> ErrorKind {
  let c = cause.to_ascii_lowercase();
  if c.contains("checksum mismatch") || c.contains("sha1") {
    ErrorKind::Checksum
  } else if c.contains("screenscraper") || c.contains("not identified") {
    ErrorKind::ScreenScraper
  } else if c.contains("servers failed")
    || c.contains("io error")
    || c.contains("http")
    || c.starts_with("media ")
  {
    ErrorKind::Download
  } else {
    ErrorKind::Other
  }
}

/// `9 checksum · 4 screenscraper · 2 download`, commonest first.
pub(crate) fn tally(causes: &[String]) -> String {
  let mut counts = [0usize; 4];
  for cause in causes {
    counts[match classify(cause) {
      ErrorKind::Checksum => 0,
      ErrorKind::ScreenScraper => 1,
      ErrorKind::Download => 2,
      ErrorKind::Other => 3,
    }] += 1;
  }

  let mut parts: Vec<(usize, &'static str)> = [
    ErrorKind::Checksum,
    ErrorKind::ScreenScraper,
    ErrorKind::Download,
    ErrorKind::Other,
  ]
  .iter()
  .enumerate()
  .filter(|&(i, _)| counts[i] > 0)
  .map(|(i, k)| (counts[i], k.label()))
  .collect();
  // Commonest first: the biggest bucket is the one worth acting on. Ties keep the
  // declaration order, so the line does not reshuffle itself as counts move.
  parts.sort_by(|a, b| b.0.cmp(&a.0));

  parts
    .iter()
    .map(|(n, label)| format!("{} {}", n, label))
    .collect::<Vec<_>>()
    .join(" · ")
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The causes the pipeline actually produces, each landing in the bucket a user would
  /// look for it in.
  #[test]
  fn the_real_causes_are_classified_where_they_belong() {
    // internetarchive, Download::verify_sha1
    assert_eq!(
      classify("Checksum mismatch: expected abc, got def"),
      ErrorKind::Checksum
    );
    // worker::helpers::lookup_failure, every variant mentions ScreenScraper
    assert_eq!(
      classify("daily ScreenScraper scrape quota exceeded"),
      ErrorKind::ScreenScraper
    );
    assert_eq!(
      classify("unexpected ScreenScraper response (HTTP 500)"),
      ErrorKind::ScreenScraper
    );
    // handle_wait_modal under --plain
    assert_eq!(
      classify("not identified — needs manual identification"),
      ErrorKind::ScreenScraper
    );
    // internetarchive, Download::fetch
    assert_eq!(
      classify("All servers failed, last error on https://ia1.example: timed out"),
      ErrorKind::Download
    );
    // handle_download_medias
    assert_eq!(
      classify("media wheel: connection reset"),
      ErrorKind::Download
    );
    // a caught panic
    assert_eq!(
      classify("panicked at src/package.rs:42: index out of bounds"),
      ErrorKind::Other
    );
  }

  /// A checksum mismatch reached through a download is a checksum problem: re-running
  /// fixes a flaky link, and never fixes a mirror serving the wrong file.
  #[test]
  fn a_checksum_failure_wins_over_the_transfer_that_carried_it() {
    assert_eq!(
      classify("media image: sha1 mismatch after HTTP 200"),
      ErrorKind::Checksum
    );
  }

  /// The tally leads with the bucket worth acting on.
  #[test]
  fn the_tally_leads_with_the_commonest_cause() {
    let causes: Vec<String> = [
      "Checksum mismatch: expected a, got b",
      "Checksum mismatch: expected c, got d",
      "media wheel: connection reset",
      "Checksum mismatch: expected e, got f",
      "daily ScreenScraper scrape quota exceeded",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(tally(&causes), "3 checksum · 1 screenscraper · 1 download");
  }

  /// Nothing failed, so the line says nothing rather than listing four zeroes.
  #[test]
  fn nothing_to_tally_says_nothing() {
    assert_eq!(tally(&[]), "");
  }
}
