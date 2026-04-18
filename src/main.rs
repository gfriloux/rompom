mod conf;
mod emulationstation;
mod package;
mod pkgbuild;

use getopts::Options;
use glob::Pattern;
use std::{env, path::Path};

use internet_archive::download::{Download, DownloadMethod};
use internet_archive::metadata::{Metadata, MetadataFile};
use screenscraper::{jeuinfo::JeuInfo, ScreenScraper};

use crate::conf::{Conf, System};
use crate::package::Package;

fn print_usage(program: &str, opts: Options) {
  let brief = format!("Usage: {} -s SYSTEM", program);
  print!("{}", opts.usage(&brief));
}

fn process_rom(conf: &Conf, system: &System, metadata: &Metadata, file: &MetadataFile) {
  let path = Path::new(&file.name);
  let filename = path.file_name().unwrap().to_str().unwrap();

  let ss = ScreenScraper::new(
    &conf.screenscraper.user.login,
    &conf.screenscraper.user.password,
    &conf.screenscraper.dev.login,
    &conf.screenscraper.dev.password,
  )
  .unwrap();

  let size = file
    .size
    .as_deref()
    .and_then(|s| s.parse::<u64>().ok())
    .unwrap_or(0);

  let ji: Option<JeuInfo> = ss
    .jeuinfo(
      system.id,
      filename,
      size,
      file.crc32.clone(),
      file.md5.clone(),
      file.sha1.clone(),
    )
    .ok();

  let urls = metadata.file_urls(&file.name).unwrap();
  let sha1 = file.sha1.clone().unwrap_or_default();

  let mut package = Package::new(ji, filename, urls.first().unwrap(), &sha1).unwrap();
  package.build(system).unwrap();

  let directory = Path::new(filename).with_extension("");
  let dest = directory.join(filename);
  let download = Download::new(metadata, &file.name).unwrap();
  if dest.exists() {
    match download.verify_sha1(&dest) {
      Ok(()) => println!("Skipping {} (already exists, checksum OK)", filename),
      Err(_) => {
        println!("Re-downloading {} (checksum mismatch)", filename);
        download.fetch(&dest, DownloadMethod::Https).unwrap();
        download.verify_sha1(&dest).unwrap();
      }
    }
  } else {
    download.fetch(&dest, DownloadMethod::Https).unwrap();
    download.verify_sha1(&dest).unwrap();
  }
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

  for item in &ia_items {
    let metadata = Metadata::get(&item.item).unwrap();
    for file in &metadata.files {
      let path = Path::new(&file.name);
      let filename = path.file_name().unwrap().to_str().unwrap();
      if !Pattern::new(&item.filter).unwrap().matches(filename) {
        println!("Skipping {}", filename);
        continue;
      }
      process_rom(&conf, &system, &metadata, file);
    }
  }
}
