extern crate reqwest;
extern crate getopts;
extern crate serde_derive;
extern crate serde_json;
extern crate checksums;
extern crate snafu;
extern crate serde_xml_rs;
extern crate chrono;
extern crate serde_yaml;
extern crate dirs;
extern crate indicatif;
extern crate screenscraper;
extern crate internet_archive;
extern crate glob;

mod conf;
mod package;
mod emulationstation;

use getopts::Options;
use std::{
   env,
   path::{
      Path,
      PathBuf
   }
};
use glob::Pattern;

use screenscraper::{ScreenScraper,jeuinfo::JeuInfo};
use internet_archive::metadata::Metadata;

use crate::package::Package;
use crate::conf::{
   Conf,
   Reference
};

fn print_usage(program: &str, opts: Options) {
   let brief = format!("Usage: {} --rom ROMFILE --id INTEGER", program);
   print!("{}", opts.usage(&brief));
}

fn main() {
  let     args: Vec<String> = env::args().collect();
  let mut opts              = Options::new();
  let     program           = args[0].clone();

  let confdir = match dirs::config_dir() {
    Some(x) => { x },
    None    => {
      eprintln!("Failed to find user configuration dir");
      return;
    }
  };


  let     conf     = Conf::load(&format!("{}/rompom.yml", confdir.display())).unwrap();

  opts.optopt ("s", "system",  "System to search for",     "SYSTEM" );
  opts.optflag("h", "help",    "print this help menu"               );

  let matches = match opts.parse(&args[1..]) {
    Ok (m) => { m }
    Err(f) => { panic!("{}", f.to_string()) }
  };

  if matches.opt_present("h") {
    print_usage(&program, opts);
    return;
  }

  let system_name = match matches.opt_str("s") {
    Some(x) => { x },
    None    => {
      print_usage(&program, opts);
      return;
    }
  };

  let system = conf.system_find(&system_name);
  for item in &system.ia_items.clone().unwrap() {
    let metadata = Metadata::get(&item.item).unwrap();

    for file in &metadata.files {
      let path = Path::new(&file.name);
      let filename = path.file_name().unwrap().to_str().unwrap();

      if ! Pattern::new(&item.filter).unwrap().matches(filename) {
        println!("Skipping {}", filename);
        continue;
      }
      let ss = ScreenScraper::new(&conf.screenscraper.user.login,
                                  &conf.screenscraper.user.password,
                                  &conf.screenscraper.dev.login,
                                  &conf.screenscraper.dev.password).unwrap();
      let res = ss.jeuinfo(system.id.clone(), filename, file.size.clone().unwrap().parse::<u64>().unwrap(), file.crc32.clone(), file.md5.clone(), file.sha1.clone());
      let ji: Option<JeuInfo> = match res {
        Ok(x)  => Some(x),
        Err(_) => None,
      };
      let mut package = Package::new(ji, &filename.to_string(), &metadata.fileurl_get(&file.name).unwrap(), &file.sha1.clone().unwrap_or("".to_string())).unwrap();
      package.build(&system).unwrap();
    }
  }
  


}
