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

/// The contents of `<system>.errors.log`: a header, then one line per failure.
///
/// One line each, tab-separated, because this file exists to be read by something other
/// than a human eye — `grep`, `cut`, a loop that re-runs the names. The cause is written
/// **whole**, unlike in the view, where it is cut to a column; that is the point of
/// writing it out at all. Newlines inside a cause are flattened for the same reason a
/// grid row flattens them: one failure has to stay one line.
pub(crate) fn log(system: &str, failures: &[(String, String)]) -> String {
  let mut out = format!("# rompom {} — {} failures\n", system, failures.len());
  for (label, cause) in failures {
    out.push_str(&format!(
      "{}\t{}\n",
      label.replace('\t', " "),
      cause.replace(['\n', '\t'], " ")
    ));
  }
  out
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

  /// One failure per line, name and cause separated by a tab, so the file can be cut
  /// and grepped rather than only read.
  #[test]
  fn the_log_is_one_line_per_failure() {
    let failures = vec![
      (
        "Bahamut Lagoon (J)".to_string(),
        "sha1 mismatch".to_string(),
      ),
      ("Front Mission (J)".to_string(), "HTTP 503".to_string()),
    ];
    assert_eq!(
      log("snes", &failures),
      "# rompom snes — 2 failures\n\
       Bahamut Lagoon (J)\tsha1 mismatch\n\
       Front Mission (J)\tHTTP 503\n"
    );
  }

  /// The whole point of the file is the cause the view had to cut — so it is written
  /// whole, and a newline inside it is flattened rather than allowed to split the line.
  #[test]
  fn the_log_keeps_the_whole_cause_on_one_line() {
    let long = "too many unrecognised ROMs today — ScreenScraper says come back tomorrow";
    let failures = vec![("Umihara Kawase".to_string(), format!("{}\nretry?", long))];
    let out = log("snes", &failures);
    assert!(out.contains(long));
    assert_eq!(out.lines().count(), 2);
  }

  /// A run with nothing to report still writes a header, so an empty file is
  /// distinguishable from one that was never written.
  #[test]
  fn an_empty_log_still_says_which_run_it_is() {
    assert_eq!(log("snes", &[]), "# rompom snes — 0 failures\n");
  }
}
