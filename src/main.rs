extern crate checksums;
extern crate chrono;
extern crate dirs;
extern crate getopts;
extern crate glob;
extern crate indicatif;
extern crate internet_archive;
extern crate reqwest;
extern crate screenscraper;
extern crate serde_derive;
extern crate serde_json;
extern crate serde_xml_rs;
extern crate serde_yaml;
extern crate snafu;

mod conf;
mod emulationstation;
mod package;

use getopts::Options;
use glob::Pattern;
use std::{env, path::Path};

use internet_archive::metadata::Metadata;
use screenscraper::{jeuinfo::JeuInfo, ScreenScraper};

use crate::conf::Conf;
use crate::package::Package;

fn print_usage(program: &str, opts: Options) {
  let brief = format!("Usage: {} --rom ROMFILE --id INTEGER", program);
  print!("{}", opts.usage(&brief));
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

  let system = conf.system_find(&system_name);
  let ia_items = match system.ia_items {
    Some(ref items) => items,
    None => {
      eprintln!(
        "System '{}' has no ia_items configured in rompom.yml",
        system_name
      );
      return;
    }
  };
  for item in ia_items {
    let metadata = Metadata::get(&item.item).unwrap();

    for file in &metadata.files {
      let path = Path::new(&file.name);
      let filename = path.file_name().unwrap().to_str().unwrap();

      if !Pattern::new(&item.filter).unwrap().matches(filename) {
        println!("Skipping {}", filename);
        continue;
      }
      let ss = ScreenScraper::new(
        &conf.screenscraper.user.login,
        &conf.screenscraper.user.password,
        &conf.screenscraper.dev.login,
        &conf.screenscraper.dev.password,
      )
      .unwrap();
      let res = ss.jeuinfo(
        system.id,
        filename,
        file.size.clone().unwrap().parse::<u64>().unwrap(),
        file.crc32.clone(),
        file.md5.clone(),
        file.sha1.clone(),
      );
      let ji: Option<JeuInfo> = res.ok();
      let urls = metadata.file_urls(&file.name).unwrap();
      let mut package = Package::new(
        ji,
        &filename.to_string(),
        urls.first().unwrap(),
        &file.sha1.clone().unwrap_or("".to_string()),
      )
      .unwrap();
      package.build(&system).unwrap();
    }
  }
}
