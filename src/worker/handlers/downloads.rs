use std::{
  fs,
  path::{Path, PathBuf},
  sync::{Arc, Mutex},
};

use checksums::{hash_file, Algorithm};
use internet_archive::download::{Download, DownloadMethod};

use crate::rom::{Rom, RomSource, StepStatus};

use super::super::{helpers::media_filename, WorkerContext};

// ── CopyRom ───────────────────────────────────────────────────────────────

/// Copy a local folder-source ROM to its output directory.
///
/// For multi-disc games, copies all disc files (disc 1 from the main source,
/// discs 2+ from `extra_discs`).
pub(crate) fn handle_copy_rom(
  rom_arc: &Arc<Mutex<Rom>>,
  _step_idx: usize,
  _ctx: &WorkerContext,
) -> Result<StepStatus, String> {
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
  fs::create_dir_all(&directory).map_err(|e| e.to_string())?;

  if rom_unchanged {
    rom_arc.lock().unwrap().bar.rom_skipped();
    return Ok(StepStatus::Done);
  }

  // Helper: copy one disc file unless it already matches the expected sha1.
  let copy_disc = |local: &Path, dest: &Path, sha1_exp: &str| -> Result<bool, String> {
    if dest.exists() {
      let actual = hash_file(dest, Algorithm::SHA1).to_lowercase();
      if actual == sha1_exp {
        return Ok(false); // already good
      }
    }
    fs::copy(local, dest).map_err(|e| e.to_string())?;
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
    rom_arc.lock().unwrap().bar.rom_done();
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
) -> Result<StepStatus, String> {
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
  fs::create_dir_all(&directory).map_err(|e| e.to_string())?;

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

  let dl1 = Download::new(&metadata, &file_name_in_item).map_err(|e| e.to_string())?;
  if dest1.exists() {
    rom_arc.lock().unwrap().bar.rom_checking();
    match dl1.verify_sha1(&dest1) {
      Ok(()) => {
        rom_arc.lock().unwrap().bar.rom_skipped();
      }
      Err(_) => {
        rom_arc.lock().unwrap().bar.rom_redownloading();
        dl1
          .fetch(&dest1, DownloadMethod::Https)
          .map_err(|e| e.to_string())?;
        dl1.verify_sha1(&dest1).map_err(|e| e.to_string())?;
        rom_arc.lock().unwrap().bar.rom_done();
      }
    }
  } else {
    rom_arc.lock().unwrap().bar.rom_downloading();
    dl1
      .fetch(&dest1, DownloadMethod::Https)
      .map_err(|e| e.to_string())?;
    dl1.verify_sha1(&dest1).map_err(|e| e.to_string())?;
    rom_arc.lock().unwrap().bar.rom_done();
  }

  // ── Extra discs (disc 2, 3, …) ────────────────────────────────────────
  for (ia_path, local_name) in &extra_discs {
    let dest = directory.join(local_name);
    let dl = Download::new(&metadata, ia_path).map_err(|e| e.to_string())?;
    if dest.exists() && dl.verify_sha1(&dest).is_ok() {
      continue; // already valid
    }
    dl.fetch(&dest, DownloadMethod::Https)
      .map_err(|e| e.to_string())?;
    dl.verify_sha1(&dest).map_err(|e| e.to_string())?;
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
) -> Result<StepStatus, String> {
  let (filename, medias) = {
    let mut rom = rom_arc.lock().unwrap();
    let filename = rom.source.filename.clone();
    let medias = rom.medias.take(); // temporarily take ownership
    (filename, medias)
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
          let needs_download =
            !dest.exists() || ctx.ss.media_download(m).verify_sha1(&dest).is_err();
          if needs_download {
            ctx
              .ss
              .media_download(m)
              .fetch(&dest)
              .map_err(|e| format!("media {}: {}", kind, e))?;
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

  // Restore medias so SaveState can record their sha1s.
  rom_arc.lock().unwrap().medias = medias;

  Ok(StepStatus::Done)
}
