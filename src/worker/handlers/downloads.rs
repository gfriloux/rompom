use std::{
  fs,
  path::Path,
  sync::{Arc, Mutex},
};

use checksums::{hash_file, Algorithm};
use internet_archive::download::{Download, DownloadMethod};

use crate::rom::{Rom, RomSource, StepStatus};

use super::super::{helpers::media_filename, WorkerContext};

// ── CopyRom ───────────────────────────────────────────────────────────────

/// Copy a local folder-source ROM to its output directory.
pub(crate) fn handle_copy_rom(
  rom_arc: &Arc<Mutex<Rom>>,
  _step_idx: usize,
  _ctx: &WorkerContext,
) -> Result<StepStatus, String> {
  let (filename, sha1_expected, local_path, rom_unchanged) = {
    let rom = rom_arc.lock().unwrap();
    let local_path = match &rom.source.source {
      RomSource::Folder(f) => f.local_path.clone(),
      _ => unreachable!("CopyRom only runs on folder sources"),
    };
    (
      rom.source.filename.clone(),
      rom.sha1.clone().unwrap_or_default(),
      local_path,
      rom.rom_unchanged,
    )
  };

  let directory = Path::new(&filename).with_extension("");
  let dest = directory.join(&filename);

  if rom_unchanged {
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    rom_arc.lock().unwrap().bar.rom_skipped();
    return Ok(StepStatus::Done);
  }

  if dest.exists() {
    rom_arc.lock().unwrap().bar.rom_checking();
    let actual = hash_file(&dest, Algorithm::SHA1).to_lowercase();
    if actual == sha1_expected {
      rom_arc.lock().unwrap().bar.rom_skipped();
    } else {
      rom_arc.lock().unwrap().bar.rom_redownloading();
      fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
      fs::copy(&local_path, &dest).map_err(|e| e.to_string())?;
      rom_arc.lock().unwrap().bar.rom_done();
    }
  } else {
    rom_arc.lock().unwrap().bar.rom_downloading();
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    fs::copy(&local_path, &dest).map_err(|e| e.to_string())?;
    rom_arc.lock().unwrap().bar.rom_done();
  }

  Ok(StepStatus::Done)
}

// ── DownloadRom ───────────────────────────────────────────────────────────

/// Download a ROM from Internet Archive to its output directory.
pub(crate) fn handle_download_rom(
  rom_arc: &Arc<Mutex<Rom>>,
  _step_idx: usize,
  _ctx: &WorkerContext,
) -> Result<StepStatus, String> {
  let (filename, file_name_in_item, metadata, rom_unchanged) = {
    let rom = rom_arc.lock().unwrap();
    let (metadata, file_name) = match &rom.source.source {
      RomSource::InternetArchive(ia) => (Arc::clone(&ia.metadata), rom.source.file_name.clone()),
      _ => unreachable!("DownloadRom only runs on IA sources"),
    };
    (
      rom.source.filename.clone(),
      file_name,
      metadata,
      rom.rom_unchanged,
    )
  };

  let directory = Path::new(&filename).with_extension("");
  let dest = directory.join(&filename);
  let download = Download::new(&metadata, &file_name_in_item).map_err(|e| e.to_string())?;

  if rom_unchanged {
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    rom_arc.lock().unwrap().bar.rom_skipped();
    return Ok(StepStatus::Done);
  }

  if dest.exists() {
    rom_arc.lock().unwrap().bar.rom_checking();
    match download.verify_sha1(&dest) {
      Ok(()) => {
        rom_arc.lock().unwrap().bar.rom_skipped();
      }
      Err(_) => {
        rom_arc.lock().unwrap().bar.rom_redownloading();
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        download
          .fetch(&dest, DownloadMethod::Https)
          .map_err(|e| e.to_string())?;
        download.verify_sha1(&dest).map_err(|e| e.to_string())?;
        rom_arc.lock().unwrap().bar.rom_done();
      }
    }
  } else {
    rom_arc.lock().unwrap().bar.rom_downloading();
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    download
      .fetch(&dest, DownloadMethod::Https)
      .map_err(|e| e.to_string())?;
    download.verify_sha1(&dest).map_err(|e| e.to_string())?;
    rom_arc.lock().unwrap().bar.rom_done();
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
