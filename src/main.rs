mod conf;
mod emulationstation;
mod package;
mod summary;
mod ui;

use getopts::Options;
use glob::Pattern;
use std::{env, path::Path, sync::Arc, thread};

use crossbeam_channel as channel;
use internet_archive::download::{Download, DownloadMethod};
use internet_archive::metadata::Metadata;
use screenscraper::{
  jeuinfo::{JeuInfo, Media},
  ScreenScraper,
};

use crate::conf::{Conf, Source};
use crate::package::{Medias, Package};
use crate::ui::{RomBar, Ui};

const N_PACK_WORKERS: usize = 4;
const N_DL_WORKERS: usize = 4;
const NAME_REGIONS: &[&str] = &["wor", "eu", "us", "fr", "jp", "ss"];

fn print_usage(program: &str, opts: Options) {
  let brief = format!("Usage: {} -s SYSTEM", program);
  print!("{}", opts.usage(&brief));
}

fn media_filename(kind: &str, format: &str) -> String {
  match kind {
    "video" => "video.mp4".to_string(),
    "manual" => "manual.pdf".to_string(),
    _ => format!("{}.{}", kind, format),
  }
}

fn download_media_if_needed(ss: &ScreenScraper, media: &Media, dest: &Path) {
  if !dest.exists() || ss.media_download(media).verify_sha1(dest).is_err() {
    ss.media_download(media).fetch(dest).unwrap();
  }
}

// Données brutes collectées avant de connaître le total (pas de bar encore)
struct FileEntry {
  metadata: Arc<Metadata>,
  file_name: String,
  filename: String,
  crc32: Option<String>,
  md5: Option<String>,
  sha1: Option<String>,
  size: u64,
  rom_url: String,
}

struct RomJob {
  bar: RomBar,
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

  opts.optopt("s", "system", "System to search for", "SYSTEM");
  opts.optflag(
    "",
    "update-config",
    "interactively update rompom.yml with missing fields",
  );
  opts.optflag("h", "help", "print this help menu");

  let matches = match opts.parse(&args[1..]) {
    Ok(m) => m,
    Err(f) => panic!("{}", f.to_string()),
  };

  if matches.opt_present("h") {
    print_usage(&program, opts);
    return;
  }

  if matches.opt_present("update-config") {
    let conf_path = format!("{}/rompom.yml", confdir.display());
    if let Err(e) = conf::Conf::update(&conf_path) {
      eprintln!("Error: {}", e);
      std::process::exit(1);
    }
    return;
  }

  let conf = match Conf::load(&format!("{}/rompom.yml", confdir.display())) {
    Ok(c) => c,
    Err(e) => {
      eprintln!("Error: {}", e);
      std::process::exit(1);
    }
  };

  let system_name = match matches.opt_str("s") {
    Some(x) => x,
    None => {
      print_usage(&program, opts);
      return;
    }
  };

  let system = match conf.find_system(&system_name) {
    Some(s) => s,
    None => {
      eprintln!("System '{}' not found in rompom.yml", system_name);
      return;
    }
  };

  let ia_items = match &system.source {
    Some(Source::InternetArchive(items)) => items.clone(),
    Some(Source::Folder(_)) => {
      eprintln!(
        "System '{}': folder source not yet implemented",
        system_name
      );
      return;
    }
    None => {
      eprintln!(
        "System '{}' has no source configured in rompom.yml",
        system_name
      );
      return;
    }
  };

  let ui = Ui::new();

  // Collecte — on ne connaît pas encore le total, pas de bars
  let mut entries: Vec<FileEntry> = Vec::new();
  for item in &ia_items {
    ui.fetching_metadata(&item.item);
    let metadata = Arc::new(Metadata::get(&item.item).unwrap());

    for file in metadata.files.iter().filter(|f| {
      let filename = Path::new(&f.name).file_name().unwrap().to_str().unwrap();
      item
        .filter
        .iter()
        .any(|pat| Pattern::new(pat).unwrap().matches(filename))
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
      entries.push(FileEntry {
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
        rom_url,
      });
    }
  }

  // Total connu — création des bars et des jobs
  let total = entries.len();
  let jobs: Vec<RomJob> = entries
    .into_iter()
    .enumerate()
    .map(|(i, e)| RomJob {
      bar: ui.new_rom_bar(i + 1, total, &e.filename),
      metadata: e.metadata,
      file_name: e.file_name,
      filename: e.filename,
      crc32: e.crc32,
      md5: e.md5,
      sha1: e.sha1,
      size: e.size,
      jeu: None,
      rom_url: e.rom_url,
      medias: Medias::default(),
      romname: String::new(),
    })
    .collect();

  // Pipeline
  let ss = ScreenScraper::new(
    &conf.screenscraper.user.login,
    &conf.screenscraper.user.password,
    &conf.screenscraper.dev.login,
    &conf.screenscraper.dev.password,
  )
  .unwrap();

  let n_disc = ss
    .user_info
    .as_ref()
    .and_then(|u| u.maxthreads.parse::<usize>().ok())
    .unwrap_or(1);

  let ss = Arc::new(ss);
  let system = Arc::new(system);
  let lang = Arc::new(conf.lang);

  let (disc_tx, disc_rx) = channel::unbounded::<RomJob>();
  let (pack_tx, pack_rx) = channel::unbounded::<RomJob>();
  let (dl_tx, dl_rx) = channel::unbounded::<RomJob>();

  for job in jobs {
    disc_tx.send(job).unwrap();
  }
  drop(disc_tx);

  // Workers discovery
  let disc_handles: Vec<_> = (0..n_disc)
    .map(|_| {
      let rx = disc_rx.clone();
      let tx = pack_tx.clone();
      let ss = Arc::clone(&ss);
      let system = Arc::clone(&system);
      thread::spawn(move || {
        for mut job in rx {
          job.bar.discovering();
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
            Some(j) => job.bar.found(&j.find_name(NAME_REGIONS)),
            None => job.bar.not_found(),
          }
          job.jeu = ji;
          job.bar.preparing_pending();
          tx.send(job).unwrap();
        }
      })
    })
    .collect();
  drop(pack_tx);

  // Workers packaging
  let pack_handles: Vec<_> = (0..N_PACK_WORKERS)
    .map(|_| {
      let rx = pack_rx.clone();
      let tx = dl_tx.clone();
      let system = Arc::clone(&system);
      let lang = Arc::clone(&lang);
      thread::spawn(move || {
        let lang_refs: Vec<&str> = lang.iter().map(|s| s.as_str()).collect();
        for mut job in rx {
          job.bar.preparing();
          let sha1 = job.sha1.clone().unwrap_or_default();
          let mut package =
            Package::new(job.jeu.take(), &job.filename, &job.rom_url, &sha1).unwrap();
          match package.build(&system, &lang_refs) {
            Ok(()) => {}
            Err(_) => {
              job.bar.finish_error();
              continue;
            }
          }
          job.romname = package.normalize_name();
          job.medias = package.medias;
          job.jeu = package.jeu;
          job.bar.downloading_pending();
          tx.send(job).unwrap();
        }
      })
    })
    .collect();
  drop(dl_tx);

  // Workers download
  let dl_handles: Vec<_> = (0..N_DL_WORKERS)
    .map(|_| {
      let rx = dl_rx.clone();
      let ss = Arc::clone(&ss);
      thread::spawn(move || {
        for job in rx {
          let directory = Path::new(&job.filename).with_extension("");
          let dest = directory.join(&job.filename);
          let download = Download::new(&job.metadata, &job.file_name).unwrap();

          if dest.exists() {
            job.bar.rom_checking();
            match download.verify_sha1(&dest) {
              Ok(()) => job.bar.rom_skipped(),
              Err(_) => {
                job.bar.rom_redownloading();
                download.fetch(&dest, DownloadMethod::Https).unwrap();
                download.verify_sha1(&dest).unwrap();
                job.bar.rom_done();
              }
            }
          } else {
            job.bar.rom_downloading();
            download.fetch(&dest, DownloadMethod::Https).unwrap();
            download.verify_sha1(&dest).unwrap();
            job.bar.rom_done();
          }

          let d = &directory;
          for (kind, maybe_media) in [
            ("video", job.medias.video.as_ref()),
            ("image", job.medias.image.as_ref()),
            ("thumbnail", job.medias.thumbnail.as_ref()),
            ("bezel", job.medias.bezel.as_ref()),
            ("marquee", job.medias.marquee.as_ref()),
            ("screenshot", job.medias.screenshot.as_ref()),
            ("wheel", job.medias.wheel.as_ref()),
            ("manual", job.medias.manual.as_ref()),
          ] {
            match maybe_media {
              Some(m) => {
                job.bar.start_media(kind);
                let dest = d.join(media_filename(kind, &m.format));
                download_media_if_needed(&ss, m, &dest);
                job.bar.media_done(kind);
              }
              None => job.bar.media_unavailable(kind),
            }
          }

          job.bar.finish();
        }
      })
    })
    .collect();

  // Attente de fin dans l'ordre du pipeline
  for h in disc_handles {
    h.join().unwrap();
  }
  for h in pack_handles {
    h.join().unwrap();
  }
  for h in dl_handles {
    h.join().unwrap();
  }

  let summary = ui.summary();
  drop(ui);
  summary.print();
}
