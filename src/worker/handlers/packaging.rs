use std::{
  path::Path,
  sync::{Arc, Mutex},
};

use crate::{
  package::{read_pkgver, Package},
  rom::{Rom, RomSource, StepStatus},
};

use super::super::{helpers::check_media_changes, WorkerContext};

// ── BuildPackage ──────────────────────────────────────────────────────────

/// Build the PKGBUILD and description.xml for a ROM.
///
/// Skips the build if neither the ROM nor any media sha1 has changed since
/// the last run (`package_unchanged = true`).
pub(crate) fn handle_build_package(
  rom_arc: &Arc<Mutex<Rom>>,
  _step_idx: usize,
  ctx: &WorkerContext,
) -> Result<StepStatus, String> {
  // Extract what we need, releasing the lock before expensive I/O.
  let (filename, disc1_filename, sha1, rom_url, extra_discs_info, jeu, rom_unchanged) = {
    let mut rom = rom_arc.lock().unwrap();
    let jeu = rom.jeu.take(); // Package::new takes ownership; we'll put it back
    let sha1 = rom.sha1.clone().unwrap_or_default();
    let (rom_url, extra_discs_info) = match &rom.source.source {
      RomSource::InternetArchive(ia) => {
        let extras: Vec<(String, String, String)> = rom
          .source
          .extra_discs
          .iter()
          .map(|d| {
            (
              d.filename.clone(),
              d.rom_url.clone(),
              d.sha1.clone().unwrap_or_default(),
            )
          })
          .collect();
        (ia.rom_url.clone(), extras)
      }
      RomSource::Folder(_) => {
        let extras: Vec<(String, String, String)> = rom
          .source
          .extra_discs
          .iter()
          .map(|d| {
            (
              d.filename.clone(),
              String::new(),
              d.sha1.clone().unwrap_or_default(),
            )
          })
          .collect();
        (String::new(), extras)
      }
    };
    // Actual disc-1 filename (differs from virtual filename for multi-disc games).
    let disc1_filename = Path::new(&rom.source.file_name)
      .file_name()
      .and_then(|n| n.to_str())
      .unwrap_or(&rom.source.filename)
      .to_string();
    (
      rom.source.filename.clone(),
      disc1_filename,
      sha1,
      rom_url,
      extra_discs_info,
      jeu,
      rom.rom_unchanged,
    )
  };

  rom_arc.lock().unwrap().bar.preparing();

  let mut package = Package::new(
    jeu,
    &filename,
    &disc1_filename,
    &rom_url,
    &sha1,
    extra_discs_info,
  )
  .map_err(|e| e.to_string())?;

  let lang_refs: Vec<&str> = ctx.lang.iter().map(|s| s.as_str()).collect();

  // Check if description.xml content would change (pure read, no I/O side effect).
  let description_changed = package.check_description_changed(&ctx.system, &lang_refs);

  // ── Delta check: skip build if ROM + all media sha1s + description are unchanged ─
  let (package_changed, debug_lines) = {
    let state = ctx.state.lock().unwrap();
    match state.roms.get(&filename) {
      None => (
        true,
        vec!["[BuildPackage] no state entry → package_changed: true".to_string()],
      ),
      Some(prev) => {
        if !rom_unchanged {
          let line = format!(
            "[BuildPackage] rom_unchanged: false → package_changed: true\n  rom sha1: state={}, current={}",
            prev.rom_sha1, sha1
          );
          (true, vec![line])
        } else {
          let (media_changed, mut lines) = check_media_changes(&package.medias, &prev.medias);
          if description_changed {
            lines.push("[BuildPackage] description.xml  : CHANGED".to_string());
          } else {
            lines.push("[BuildPackage] description.xml  : ok       (unchanged)".to_string());
          }
          if media_changed || description_changed {
            lines.push(
              "[BuildPackage] → package_changed: true (media or description mismatch above)"
                .to_string(),
            );
            (true, lines)
          } else {
            lines.push("[BuildPackage] → package_unchanged: true".to_string());
            (false, lines)
          }
        }
      }
    }
  };
  rom_arc.lock().unwrap().debug_log.extend(debug_lines);

  if package_changed {
    let dir = Path::new(&filename).with_extension("");
    let pkgver = read_pkgver(&dir) + 1;
    package
      .build(&ctx.system, &lang_refs, pkgver)
      .map_err(|e| e.to_string())?;
  }

  // Show description.xml icon: green if written/updated, gray if unchanged.
  if description_changed {
    rom_arc.lock().unwrap().bar.media_done("description");
  } else {
    rom_arc.lock().unwrap().bar.media_skipped("description");
  }

  // Move results back into rom.
  let romname = package.normalize_name();
  let medias = package.medias;
  {
    let mut rom = rom_arc.lock().unwrap();
    rom.jeu = package.jeu;
    rom.medias = Some(medias);
    rom.romname = Some(romname);
    rom.package_unchanged = !package_changed;
  }

  rom_arc.lock().unwrap().bar.downloading_pending();
  Ok(StepStatus::Done)
}
