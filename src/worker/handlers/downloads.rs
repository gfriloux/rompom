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

use super::super::{
  helpers::{is_not_found, media_failure},
  WorkerContext,
};

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

// ── Putting the discs in place ────────────────────────────────────────────

/// One disc to put in the ROM's output directory, and what that means for this source.
///
/// The two handlers differ in exactly these two respects: a folder source compares the
/// file's sha1 with the one `ComputeHashes` produced and copies it, an Internet Archive
/// source lets the library verify and transfers it. Everything around them — the skip
/// when nothing has changed, disc 1 driving the row while the extra discs go quietly,
/// the check before re-fetching — was written out twice.
struct Disc<'a> {
  /// File name inside the output directory.
  name: String,
  /// Whether what is already there is the file we want.
  is_valid: Box<dyn Fn(&Path) -> bool + 'a>,
  /// Put it there, and make sure of what landed.
  fetch: Fetch<'a>,
}

/// Writes one disc to `dest`, and answers for what ended up there.
type Fetch<'a> = Box<dyn Fn(&Path) -> Result<(), StepError> + 'a>;

/// Puts each disc in place, fetching only what is missing or wrong.
///
/// Only disc 1 touches the bar: the grid has one `rom` cell per ROM, not one per disc.
/// `placed` is called for it alone, and only when something was actually written — the
/// two handlers count a transfer differently, one by the bytes that arrived and the
/// other by the size of the file `fs::copy` left behind.
fn place_discs(
  rom_arc: &Arc<Mutex<Rom>>,
  directory: &Path,
  discs: &[Disc<'_>],
  placed: impl Fn(&Path),
) -> Result<(), StepError> {
  for (index, disc) in discs.iter().enumerate() {
    let dest = directory.join(&disc.name);
    let present = dest.exists();
    let leads = index == 0;
    // A cheap second handle, so the row can be updated without holding the `Rom`.
    let bar = || rom_arc.lock().unwrap().bar.handle();

    if leads {
      if present {
        bar().rom_checking();
      } else {
        bar().rom_downloading();
      }
    }

    if present && (disc.is_valid)(&dest) {
      if leads {
        bar().rom_skipped();
      }
      continue;
    }

    if leads && present {
      bar().rom_redownloading();
    }
    (disc.fetch)(&dest)?;
    if leads {
      placed(&dest);
    }
  }
  Ok(())
}

// ── CopyRom ───────────────────────────────────────────────────────────────

/// A disc that is already on this machine, checked against the sha1 `ComputeHashes`
/// produced for it.
///
/// An unreadable destination is not a reason to fail: it reads as invalid, and gets
/// rewritten.
fn local_disc<'a>(local: &'a Path, name: String, sha1: &'a str) -> Disc<'a> {
  Disc {
    name,
    is_valid: Box::new(move |dest: &Path| sha1_file(dest).is_ok_and(|actual| actual == sha1)),
    fetch: Box::new(move |dest: &Path| {
      fs::copy(local, dest)
        .map(|_| ())
        .map_err(StepError::transient)
    }),
  }
}

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

  // Disc 1 keeps its own basename, which for a multi-disc game is not the virtual one.
  let disc1_local_name = local_path
    .file_name()
    .map(|n| n.to_string_lossy().into_owned())
    .unwrap_or_else(|| filename.clone());

  let mut discs = vec![local_disc(&local_path, disc1_local_name, &sha1_expected)];
  for (extra_local, extra_filename, extra_sha1) in &extra_discs {
    discs.push(local_disc(extra_local, extra_filename.clone(), extra_sha1));
  }

  place_discs(rom_arc, &directory, &discs, |dest| {
    // `fs::copy` never calls back, so the file it left behind is the only measure.
    rom_arc.lock().unwrap().bar.rom_copied(written(dest));
  })?;

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

  // Disc 1's own basename comes from its IA path: for a multi-disc game the virtual
  // `filename` has had the disc indicator taken out of it.
  let disc1_local_name = Path::new(&file_name_in_item)
    .file_name()
    .map(|n| n.to_string_lossy().into_owned())
    .unwrap_or_else(|| filename.clone());

  // Every `Download` is built up front: they borrow the item metadata, and the closures
  // below borrow them in turn.
  let downloads: Vec<Download<'_>> = std::iter::once(&file_name_in_item)
    .chain(extra_discs.iter().map(|(ia_path, _)| ia_path))
    .map(|path| Download::new(&metadata, path))
    .collect::<Result<_, _>>()
    .map_err(StepError::transient)?;

  let names = std::iter::once(disc1_local_name)
    .chain(extra_discs.iter().map(|(_, local_name)| local_name.clone()));
  let discs: Vec<Disc<'_>> = names
    .zip(&downloads)
    .map(|(name, dl)| Disc {
      name,
      is_valid: Box::new(move |dest: &Path| dl.verify_sha1(dest).is_ok()),
      // Verified again after the transfer: what arrived is not necessarily what the
      // item metadata promised, and a truncated ROM must fail the step, not be kept.
      fetch: Box::new(move |dest: &Path| {
        fetch_with_progress(dl, dest, rom_arc)?;
        dl.verify_sha1(dest).map_err(StepError::transient)
      }),
    })
    .collect();

  place_discs(rom_arc, &directory, &discs, |_| {
    // The bytes were counted by the progress callback as they arrived.
    rom_arc.lock().unwrap().bar.rom_done();
  })?;

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
    for (kind, maybe_media) in medias.iter() {
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
            let fetch = |media: &Media| {
              let bar = rom_arc.lock().unwrap().bar.handle();
              ctx
                .ss
                .media_download(media)
                .fetch_with_progress(&dest, |read, total| bar.media_progress(read, total))
            };
            // A 404 means the public path has no such file, and no retry will change
            // that. `m` still holds the `mediaJeu.php` call ScreenScraper handed back,
            // which always works — at the price of a request against the account, hence
            // only here and only on a 404. Nothing was written yet: the library checks
            // the status before it creates the file.
            //
            // The PKGBUILD keeps the public URL. It cannot carry this one, which has the
            // credentials in it, and the asset is installed from the file sitting next to
            // the PKGBUILD anyway — but a `makepkg` in a clean directory will fail on it.
            let outcome = match fetch(&direct) {
              Err(ref e) if is_not_found(e) => {
                rom_arc.lock().unwrap().debug_log.push(format!(
                  "[DownloadMedias] media {:<12}: public path 404 → fetched through the API",
                  kind
                ));
                fetch(m)
              }
              first => first,
            };
            outcome.map_err(|e| media_failure(kind, &e))?;
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
