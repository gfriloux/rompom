mod conf;
mod emulationstation;
mod package;
mod pkgbuild;
mod ui;

use getopts::Options;
use glob::Pattern;
use std::{
  env,
  path::{Path, PathBuf},
  sync::Arc,
};

use internet_archive::download::{Download, DownloadMethod};
use internet_archive::metadata::Metadata;
use screenscraper::{
  jeuinfo::{JeuInfo, Media},
  ScreenScraper,
};

use crate::conf::Conf;
use crate::package::{Medias, Package};
use crate::ui::Ui;

fn print_usage(program: &str, opts: Options) {
  let brief = format!("Usage: {} -s SYSTEM", program);
  print!("{}", opts.usage(&brief));
}

fn game_label(jeu: &Option<JeuInfo>, filename: &str) -> String {
  let v = ["fr", "eu", "en", "us", "wor", "jp", "ss"];
  jeu
    .as_ref()
    .map(|j| j.find_name(&v))
    .filter(|n| !n.is_empty() && n != "Unknown")
    .unwrap_or_else(|| filename.to_string())
}

fn download_media_if_needed(ss: &ScreenScraper, media: &Media, dest: &Path) {
  if !dest.exists() || ss.media_download(media).verify_sha1(dest).is_err() {
    ss.media_download(media).fetch(dest).unwrap();
  }
}

struct RomJob {
  metadata: Arc<Metadata>,
  file_name: String,
  filename: String,
  crc32: Option<String>,
  md5: Option<String>,
  sha1: Option<String>,
  size: u64,
  jeu: Option<JeuInfo>,
  rom_url: String,
  medias: Medias,
  romname: String,
}

fn main() {
  let args: Vec<String> = env::args().collect();
  let mut opts = Options::new();
  let program = args[0].clone();

  let confdir = match dirs::config_dir() {
    Some(x) => x,
    None => {
      eprintln!("Failed to find user configuration dir");
      return;
    }
  };

  let conf = Conf::load(&format!("{}/rompom.yml", confdir.display())).unwrap();

  opts.optopt("s", "system", "System to search for", "SYSTEM");
  opts.optflag("h", "help", "print this help menu");

  let matches = match opts.parse(&args[1..]) {
    Ok(m) => m,
    Err(f) => {
      panic!("{}", f.to_string())
    }
  };

  if matches.opt_present("h") {
    print_usage(&program, opts);
    return;
  }

  let system_name = match matches.opt_str("s") {
    Some(x) => x,
    None => {
      print_usage(&program, opts);
      return;
    }
  };

  let system = match conf.system_find(&system_name) {
    Some(s) => s,
    None => {
      eprintln!("System '{}' not found in rompom.yml", system_name);
      return;
    }
  };

  let ia_items = match system.ia_items {
    Some(ref items) => items.clone(),
    None => {
      eprintln!(
        "System '{}' has no ia_items configured in rompom.yml",
        system_name
      );
      return;
    }
  };

  let ui = Ui::new();

  // Collecte — construit la liste de jobs
  let mut jobs: Vec<RomJob> = Vec::new();
  for item in &ia_items {
    ui.fetching_metadata(&item.item);
    let metadata = Arc::new(Metadata::get(&item.item).unwrap());

    for file in metadata.files.iter().filter(|f| {
      let filename = Path::new(&f.name).file_name().unwrap().to_str().unwrap();
      Pattern::new(&item.filter).unwrap().matches(filename)
    }) {
      let filename = Path::new(&file.name)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
      let rom_url = metadata
        .file_urls(&file.name)
        .unwrap()
        .into_iter()
        .next()
        .unwrap_or_default();
      jobs.push(RomJob {
        metadata: Arc::clone(&metadata),
        file_name: file.name.clone(),
        filename,
        crc32: file.crc32.clone(),
        md5: file.md5.clone(),
        sha1: file.sha1.clone(),
        size: file
          .size
          .as_deref()
          .and_then(|s| s.parse().ok())
          .unwrap_or(0),
        jeu: None,
        rom_url,
        medias: Medias::default(),
        romname: String::new(),
      });
    }
  }

  let total = jobs.len();

  // Phase 1 — Découverte (ScreenScraper)
  ui.phase_discovery(total);
  let ss = ScreenScraper::new(
    &conf.screenscraper.user.login,
    &conf.screenscraper.user.password,
    &conf.screenscraper.dev.login,
    &conf.screenscraper.dev.password,
  )
  .unwrap();

  for (i, job) in jobs.iter_mut().enumerate() {
    let progress = ui.rom_discovery(i + 1, total, &job.filename);
    let ji = ss
      .jeuinfo(
        system.id,
        &job.filename,
        job.size,
        job.crc32.clone(),
        job.md5.clone(),
        job.sha1.clone(),
      )
      .ok();
    match &ji {
      Some(j) => {
        progress.screenscraper_found(&j.find_name(&["fr", "eu", "en", "us", "wor", "jp", "ss"]))
      }
      None => progress.screenscraper_not_found(),
    }
    job.jeu = ji;
  }

  // Phase 2 — Packaging
  ui.phase_packaging(total);
  for (i, job) in jobs.iter_mut().enumerate() {
    let label = game_label(&job.jeu, &job.filename);
    let progress = ui.packaging_progress(i + 1, total, &label);
    let sha1 = job.sha1.clone().unwrap_or_default();
    let mut package = Package::new(job.jeu.take(), &job.filename, &job.rom_url, &sha1).unwrap();
    match package.build(&system) {
      Ok(()) => progress.done(),
      Err(_) => progress.error(),
    }
    job.romname = package.name_normalize();
    job.medias = package.medias;
    job.jeu = package.jeu;
  }

  // Phase 3 — Downloads
  ui.phase_downloads(total);
  for (i, job) in jobs.iter().enumerate() {
    let label = game_label(&job.jeu, &job.filename);
    let progress = ui.download_progress(i + 1, total, &label);
    let directory = Path::new(&job.filename).with_extension("");

    // ROM
    let dest = directory.join(&job.filename);
    let download = Download::new(&job.metadata, &job.file_name).unwrap();
    if dest.exists() {
      progress.rom_checking();
      match download.verify_sha1(&dest) {
        Ok(()) => progress.rom_skipped(),
        Err(_) => {
          progress.rom_redownloading();
          download.fetch(&dest, DownloadMethod::Https).unwrap();
          download.verify_sha1(&dest).unwrap();
          progress.rom_done();
        }
      }
    } else {
      progress.rom_downloading();
      download.fetch(&dest, DownloadMethod::Https).unwrap();
      download.verify_sha1(&dest).unwrap();
      progress.rom_done();
    }

    // Médias
    let d = &directory;
    let media_entries: Vec<(&str, Option<&Media>, PathBuf)> = vec![
      ("video", job.medias.video.as_ref(), d.join("video.mp4")),
      (
        "image",
        job.medias.image.as_ref(),
        job
          .medias
          .image
          .as_ref()
          .map_or_else(PathBuf::new, |m| d.join(format!("image.{}", m.format))),
      ),
      (
        "thumbnail",
        job.medias.thumbnail.as_ref(),
        job
          .medias
          .thumbnail
          .as_ref()
          .map_or_else(PathBuf::new, |m| d.join(format!("thumbnail.{}", m.format))),
      ),
      (
        "bezel",
        job.medias.bezel.as_ref(),
        job
          .medias
          .bezel
          .as_ref()
          .map_or_else(PathBuf::new, |m| d.join(format!("bezel.{}", m.format))),
      ),
      (
        "marquee",
        job.medias.marquee.as_ref(),
        job
          .medias
          .marquee
          .as_ref()
          .map_or_else(PathBuf::new, |m| d.join(format!("marquee.{}", m.format))),
      ),
      (
        "screenshot",
        job.medias.screenshot.as_ref(),
        job
          .medias
          .screenshot
          .as_ref()
          .map_or_else(PathBuf::new, |m| d.join(format!("screenshot.{}", m.format))),
      ),
      (
        "wheel",
        job.medias.wheel.as_ref(),
        job
          .medias
          .wheel
          .as_ref()
          .map_or_else(PathBuf::new, |m| d.join(format!("wheel.{}", m.format))),
      ),
      ("manual", job.medias.manual.as_ref(), d.join("manual.pdf")),
    ];

    for (kind, maybe_media, dest) in &media_entries {
      match maybe_media {
        Some(m) => {
          progress.start_media(kind);
          download_media_if_needed(&ss, m, dest);
          progress.media_done(kind);
        }
        None => progress.media_unavailable(kind),
      }
    }

    progress.finish();
  }
}
