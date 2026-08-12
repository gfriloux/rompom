use std::{
  fs,
  path::{Path, PathBuf},
  sync::{Arc, Mutex},
};

use internet_archive::download::{Download, DownloadMethod};

use screenscraper::jeuinfo::Media;

use crate::{
  hash::sha1_file,
  package::{media_filename, media_url},
  rom::{Rom, RomSource, StepError, StepStatus},
};

use super::super::{helpers::media_failure, WorkerContext};

/// Downloads one disc, reporting progress to the ROM's bar as the bytes land.
///
/// The bar is cloned out of the `Rom` rather than borrowed through it: the callback runs
/// for the whole transfer, and holding the `Rom` lock that long would block every other
/// worker that wants to read this ROM — including the renderer's own reads through the
/// shared `AppState`.
fn fetch_with_progress(
  dl: &Download<'_>,
  dest: &Path,
  rom_arc: &Arc<Mutex<Rom>>,
) -> Result<(), StepError> {
  let bar = rom_arc.lock().unwrap().bar.handle();
  dl.fetch_with_progress(dest, DownloadMethod::Https, |read, total| {
    bar.rom_progress(read, total)
  })
  .map_err(StepError::transient)
}

/// Size of a file that was just written, for the run's transfer volume.
///
/// The file on disk is the only measure available: neither `internetarchive` nor
/// `screenscraper` reports anything while a download is in flight, so the volume
/// advances one finished file at a time. A stat that fails contributes nothing rather
/// than failing the step — this figure is a display, not a decision.
fn written(path: &Path) -> u64 {
  fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

// ── CopyRom ───────────────────────────────────────────────────────────────

/// Copy a local folder-source ROM to its output directory.
///
/// For multi-disc games, copies all disc files (disc 1 from the main source,
/// discs 2+ from `extra_discs`).
pub(crate) fn handle_copy_rom(
  rom_arc: &Arc<Mutex<Rom>>,
  _step_idx: usize,
  _ctx: &WorkerContext,
) -> Result<StepStatus, StepError> {
  let (filename, sha1_expected, local_path, rom_unchanged, extra_discs) = {
    let rom = rom_arc.lock().unwrap();
    let local_path = match &rom.source.source {
      RomSource::Folder(f) => f.local_path.clone(),
      _ => unreachable!("CopyRom only runs on folder sources"),
    };
    // Extra disc local paths and sha1s (computed by ComputeHashes).
    let extra_discs: Vec<(PathBuf, String, String)> = rom
      .source
      .extra_discs
      .iter()
      .zip(rom.extra_disc_sha1s.iter())
      .map(|(disc, sha1)| {
        (
          disc.local_path.clone().unwrap_or_default(),
          disc.filename.clone(),
          sha1.clone(),
        )
      })
      .collect();
    (
      rom.source.filename.clone(),
      rom.sha1.clone().unwrap_or_default(),
      local_path,
      rom.rom_unchanged,
      extra_discs,
    )
  };

  // Output directory is derived from the logical/virtual filename.
  let directory = Path::new(&filename).with_extension("");
  fs::create_dir_all(&directory).map_err(StepError::transient)?;

  if rom_unchanged {
    rom_arc.lock().unwrap().bar.rom_skipped();
    return Ok(StepStatus::Done);
  }

  // Helper: copy one disc file unless it already matches the expected sha1.
  let copy_disc = |local: &Path, dest: &Path, sha1_exp: &str| -> Result<bool, StepError> {
    if dest.exists() {
      // An unreadable destination is not a reason to fail: fall through and rewrite it.
      if sha1_file(dest).is_ok_and(|actual| actual == sha1_exp) {
        return Ok(false); // already good
      }
    }
    fs::copy(local, dest).map_err(StepError::transient)?;
    Ok(true) // copied
  };

  // ── Disc 1 ────────────────────────────────────────────────────────────
  // Use the actual disc-1 filename (may differ from virtual filename for multi-disc).
  let disc1_local_name = local_path
    .file_name()
    .map(|n| n.to_string_lossy().into_owned())
    .unwrap_or_else(|| filename.clone());
  let dest1 = directory.join(&disc1_local_name);

  if dest1.exists() {
    rom_arc.lock().unwrap().bar.rom_checking();
  } else {
    rom_arc.lock().unwrap().bar.rom_downloading();
  }
  let updated = copy_disc(&local_path, &dest1, &sha1_expected)?;
  if updated {
    rom_arc.lock().unwrap().bar.rom_copied(written(&dest1));
  } else {
    rom_arc.lock().unwrap().bar.rom_skipped();
  }

  // ── Extra discs (disc 2, 3, …) ────────────────────────────────────────
  for (extra_local, extra_filename, extra_sha1) in &extra_discs {
    let dest = directory.join(extra_filename);
    copy_disc(extra_local, &dest, extra_sha1)?;
  }

  Ok(StepStatus::Done)
}

// ── DownloadRom ───────────────────────────────────────────────────────────

/// Download a ROM from Internet Archive to its output directory.
///
/// For multi-disc games, downloads all disc files (disc 1 from the main
/// source, discs 2+ from `extra_discs`).
pub(crate) fn handle_download_rom(
  rom_arc: &Arc<Mutex<Rom>>,
  _step_idx: usize,
  _ctx: &WorkerContext,
) -> Result<StepStatus, StepError> {
  let (filename, file_name_in_item, metadata, rom_unchanged, extra_discs) = {
    let rom = rom_arc.lock().unwrap();
    let (metadata, file_name) = match &rom.source.source {
      RomSource::InternetArchive(ia) => (Arc::clone(&ia.metadata), rom.source.file_name.clone()),
      _ => unreachable!("DownloadRom only runs on IA sources"),
    };
    let extra_discs: Vec<(String, String)> = rom
      .source
      .extra_discs
      .iter()
      .map(|d| (d.file_name.clone(), d.filename.clone()))
      .collect();
    (
      rom.source.filename.clone(),
      file_name,
      metadata,
      rom.rom_unchanged,
      extra_discs,
    )
  };

  // Output directory derived from the logical/virtual filename.
  let directory = Path::new(&filename).with_extension("");
  fs::create_dir_all(&directory).map_err(StepError::transient)?;

  if rom_unchanged {
    rom_arc.lock().unwrap().bar.rom_skipped();
    return Ok(StepStatus::Done);
  }

  // ── Disc 1 ────────────────────────────────────────────────────────────
  // Derive the actual local filename from the IA path (handles multi-disc
  // where the virtual `filename` differs from the disc-1 basename).
  let disc1_local_name = Path::new(&file_name_in_item)
    .file_name()
    .map(|n| n.to_string_lossy().into_owned())
    .unwrap_or_else(|| filename.clone());
  let dest1 = directory.join(&disc1_local_name);

  let dl1 = Download::new(&metadata, &file_name_in_item).map_err(StepError::transient)?;
  if dest1.exists() {
    rom_arc.lock().unwrap().bar.rom_checking();
    match dl1.verify_sha1(&dest1) {
      Ok(()) => {
        rom_arc.lock().unwrap().bar.rom_skipped();
      }
      Err(_) => {
        rom_arc.lock().unwrap().bar.rom_redownloading();
        fetch_with_progress(&dl1, &dest1, rom_arc)?;
        dl1.verify_sha1(&dest1).map_err(StepError::transient)?;
        rom_arc.lock().unwrap().bar.rom_done();
      }
    }
  } else {
    rom_arc.lock().unwrap().bar.rom_downloading();
    fetch_with_progress(&dl1, &dest1, rom_arc)?;
    dl1.verify_sha1(&dest1).map_err(StepError::transient)?;
    rom_arc.lock().unwrap().bar.rom_done();
  }

  // ── Extra discs (disc 2, 3, …) ────────────────────────────────────────
  for (ia_path, local_name) in &extra_discs {
    let dest = directory.join(local_name);
    let dl = Download::new(&metadata, ia_path).map_err(StepError::transient)?;
    if dest.exists() && dl.verify_sha1(&dest).is_ok() {
      continue; // already valid
    }
    fetch_with_progress(&dl, &dest, rom_arc)?;
    dl.verify_sha1(&dest).map_err(StepError::transient)?;
  }

  Ok(StepStatus::Done)
}

// ── DownloadMedias ────────────────────────────────────────────────────────

/// Download all available media assets for a ROM.
///
/// Iterates over the 8 canonical media types in order. Already-valid files
/// are skipped (sha1 verified). Updates the bar icons for each type.
///
/// Takes `rom.medias` out temporarily to avoid holding the Rom lock during
/// downloads, then puts it back on completion.
pub(crate) fn handle_download_medias(
  rom_arc: &Arc<Mutex<Rom>>,
  _step_idx: usize,
  ctx: &WorkerContext,
) -> Result<StepStatus, StepError> {
  let (filename, medias, jeu_id) = {
    let rom = rom_arc.lock().unwrap();
    let filename = rom.source.filename.clone();
    // Cloned, not taken. A `?` further down returns without restoring, and because a
    // Transient error re-runs *this same step*, the retry then found `rom.medias` empty,
    // skipped the whole loop and reported success — a ROM marked done with no media and
    // its dots frozen wherever the first attempt stopped.
    let medias = rom.medias.clone();
    // The game ID the public media path is built from. Absent only when the user skipped
    // identification — in which case there are no medias to fetch either.
    let jeu_id = rom.jeu.as_ref().map(|j| j.id.clone()).unwrap_or_default();
    (filename, medias, jeu_id)
  };

  let directory = Path::new(&filename).with_extension("");

  if let Some(ref medias) = medias {
    for (kind, maybe_media) in [
      ("video", medias.video.as_ref()),
      ("image", medias.image.as_ref()),
      ("thumbnail", medias.thumbnail.as_ref()),
      ("bezel", medias.bezel.as_ref()),
      ("marquee", medias.marquee.as_ref()),
      ("screenshot", medias.screenshot.as_ref()),
      ("wheel", medias.wheel.as_ref()),
      ("manual", medias.manual.as_ref()),
    ] {
      match maybe_media {
        Some(m) => {
          rom_arc.lock().unwrap().bar.start_media(kind);
          let dest = directory.join(media_filename(kind, &m.format));
          // Fetched from the public path, never from the `mediaJeu.php` URL the API
          // handed back: pulling every asset of every ROM through the API is how an
          // account gets rate-limited off ScreenScraper. Same link the PKGBUILD carries,
          // built by the same function.
          let direct = Media {
            url: media_url(ctx.system.id, &jeu_id, m),
            ..m.clone()
          };
          let needs_download =
            !dest.exists() || ctx.ss.media_download(&direct).verify_sha1(&dest).is_err();
          if needs_download {
            let bar = rom_arc.lock().unwrap().bar.handle();
            ctx
              .ss
              .media_download(&direct)
              .fetch_with_progress(&dest, |read, total| bar.media_progress(read, total))
              .map_err(|e| media_failure(kind, &e))?;
            rom_arc.lock().unwrap().bar.media_done(kind);
          } else {
            rom_arc.lock().unwrap().bar.media_skipped(kind);
          }
        }
        None => {
          rom_arc.lock().unwrap().bar.media_unavailable(kind);
        }
      }
    }
  }

  Ok(StepStatus::Done)
}
