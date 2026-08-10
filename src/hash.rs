//! File hashing — SHA1, MD5 and CRC32 over a streamed file.
//!
//! These three values decide whether a ROM has changed, so they decide whether the run
//! re-downloads a library and bumps every `pkgver`. They used to come from the
//! `checksums` crate, dropped for two reasons:
//!
//! - it pulls `shaman` → `rustc-serialize 0.3.25` (RUSTSEC-2022-0004), unfixed upstream
//!   and unfixable from here, which is why `.cargo/audit.toml` had to ignore it;
//! - `hash_file` panics on an unreadable file. A panic in a step handler is caught and
//!   turned into a failed ROM, so nothing was lost — but "cannot read this file" is an
//!   ordinary `io::Error` and reads far better as one.
//!
//! Hex is lowercase here. Every caller of the old API appended `.to_lowercase()`, except
//! the PKGBUILD path where `sanitize_sha1` lowercases anyway.

use std::{
  fs::File,
  io::{self, Read},
  path::Path,
};

use crc32fast::Hasher as Crc32;
use md5::Md5;
use sha1::{Digest, Sha1};

/// 64 KiB: large enough that a multi-gigabyte `.chd` is not read syscall by syscall,
/// small enough to stay off the stack of nine worker threads.
const CHUNK: usize = 64 * 1024;

/// Feeds a file to `sink` in chunks, so a 40 GB disc image never sits in memory.
fn stream(path: &Path, mut sink: impl FnMut(&[u8])) -> io::Result<()> {
  let mut file = File::open(path)?;
  let mut buf = vec![0u8; CHUNK];
  loop {
    let n = file.read(&mut buf)?;
    if n == 0 {
      return Ok(());
    }
    sink(&buf[..n]);
  }
}

fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn sha1_file(path: &Path) -> io::Result<String> {
  let mut hasher = Sha1::new();
  stream(path, |chunk| hasher.update(chunk))?;
  Ok(hex(&hasher.finalize()))
}

pub fn md5_file(path: &Path) -> io::Result<String> {
  let mut hasher = Md5::new();
  stream(path, |chunk| hasher.update(chunk))?;
  Ok(hex(&hasher.finalize()))
}

/// Zero-padded to eight digits. ScreenScraper matches CRC32 as a string, so a value
/// under `0x10000000` printed without its leading zero simply never matches.
pub fn crc32_file(path: &Path) -> io::Result<String> {
  let mut hasher = Crc32::new();
  stream(path, |chunk| hasher.update(chunk))?;
  Ok(format!("{:08x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::Write;

  fn tmp_file(contents: &[u8]) -> (tempdir::Guard, std::path::PathBuf) {
    let guard = tempdir::Guard::new();
    let path = guard.path().join("payload");
    let mut f = File::create(&path).unwrap();
    f.write_all(contents).unwrap();
    (guard, path)
  }

  /// The published vectors. A hash function that is merely self-consistent would pass
  /// a round-trip test and still corrupt every sha1sums line in every PKGBUILD.
  #[test]
  fn the_published_vectors_come_out_right() {
    let (_g, empty) = tmp_file(b"");
    assert_eq!(
      sha1_file(&empty).unwrap(),
      "da39a3ee5e6b4b0d3255bfef95601890afd80709"
    );
    assert_eq!(
      md5_file(&empty).unwrap(),
      "d41d8cd98f00b204e9800998ecf8427e"
    );
    assert_eq!(crc32_file(&empty).unwrap(), "00000000");

    let (_g, abc) = tmp_file(b"abc");
    assert_eq!(
      sha1_file(&abc).unwrap(),
      "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    assert_eq!(md5_file(&abc).unwrap(), "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(crc32_file(&abc).unwrap(), "352441c2");
  }

  /// ROMs are read in 64 KiB chunks. A hasher fed one chunk at a time must produce what
  /// it would over the whole buffer — this is the bug that would only ever show up on
  /// files big enough that nobody hashes them by hand to check.
  #[test]
  fn a_file_larger_than_one_chunk_hashes_the_same_as_one_shot() {
    let payload: Vec<u8> = (0..CHUNK * 2 + 517).map(|i| (i % 251) as u8).collect();
    let (_g, path) = tmp_file(&payload);

    let mut oneshot = Sha1::new();
    oneshot.update(&payload);

    assert_eq!(sha1_file(&path).unwrap(), hex(&oneshot.finalize()));
  }

  /// CRC32 under 0x10000000 must keep its leading zero: ScreenScraper compares the
  /// string, so a seven-digit value matches nothing and the ROM looks unknown.
  #[test]
  fn a_small_crc32_keeps_its_leading_zero() {
    // "\0\0\0\0" hashes to 0x2144df1c; find a payload below 0x10000000 instead.
    let (_g, path) = tmp_file(b"rompom");
    let value = crc32_file(&path).unwrap();
    assert_eq!(value.len(), 8, "got: {}", value);

    let mut expect = Crc32::new();
    expect.update(b"rompom");
    assert_eq!(value, format!("{:08x}", expect.finalize()));
  }

  /// The old `hash_file` panicked here. A missing or unreadable file is an ordinary
  /// mistake — a ROM deleted mid-run, a network mount gone — and it must arrive as an
  /// error the handler can report, not as an unwind.
  #[test]
  fn an_unreadable_file_is_an_error_not_a_panic() {
    let missing = Path::new("/nonexistent/rompom/no-such-rom.zip");
    assert!(sha1_file(missing).is_err());
    assert!(md5_file(missing).is_err());
    assert!(crc32_file(missing).is_err());
  }

  /// Minimal scratch directory, removed on drop. Pulling in a dev-dependency for this
  /// would mean another crate in the audit surface for four lines of code.
  mod tempdir {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    pub struct Guard(PathBuf);

    impl Guard {
      pub fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
          std::env::temp_dir().join(format!("rompom-hash-test-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).unwrap();
        Guard(dir)
      }

      pub fn path(&self) -> &Path {
        &self.0
      }
    }

    impl Drop for Guard {
      fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
      }
    }
  }
}
