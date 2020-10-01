             extern crate reqwest;
             extern crate getopts;
#[macro_use] extern crate serde_derive;
             extern crate serde_json;
             extern crate checksums;
             extern crate snafu;
             extern crate serde_xml_rs;
             extern crate chrono;
             extern crate serde_yaml;
             extern crate dirs;
             extern crate indicatif;

mod conf;
mod jeuinfos;
mod screenscraper;
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
use indicatif::{
   ProgressBar,
   ProgressStyle
};

use crate::jeuinfos::JeuInfos;
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
   let     systemid;
   let     rom;
   let     id;
   let     matches;
   let     pkgname;
   let     name;
   let     confdir;
   let mut package;
   let     system;
   let     jeuinfos;
   let     hash;
   let     pb;

   pb  = ProgressBar::new(3);

   let sty = ProgressStyle::default_bar().template("{spinner:.green} {pos:>7}/{len:7} {prefix:.bold}▕{bar:.blue}| {wide_msg}").progress_chars("█▇▆▅▄▃▂▁  ");
   pb.set_style(sty.clone());

   confdir = match dirs::config_dir() {
      Some(x) => { x },
      None    => {
         eprintln!("Failed to find user configuration dir");
         return;
      }
   };


   let     conf     = Conf::load(&format!("{}/rompom.yml", confdir.display())).unwrap();

   opts.optopt ("s", "system",  "System to search for",     "SYSTEM" );
   opts.optopt ("r", "rom",     "Rom file to package",      "ROM"    );
   opts.optopt ("i", "id",      "Game ID on Screenscraper", "ID"     );
   opts.optopt ("n", "name",    "Game name",                "NAME"   );
   opts.optopt ("p", "package", "Package name",             "PACKAGE");
   opts.optflag("h", "help",    "print this help menu"               );
   opts.optflag("u", "update",  "Update package"                     );

   matches = match opts.parse(&args[1..]) {
      Ok (m) => { m }
      Err(f) => { panic!(f.to_string()) }
   };

   if matches.opt_present("h") {
      print_usage(&program, opts);
      return;
   }

   if matches.opt_present("u") {
      let reference = Reference::load(&"./rompom.yml".to_string()).unwrap();
      system        = conf.system_find(reference.systemid);
      hash          = checksums::hash_file(Path::new(&reference.gamerom), checksums::Algorithm::SHA1);

      pb.set_message(&format!("Fetching game infos"));
      jeuinfos      = JeuInfos::get(&conf,
                                    &system,
                                    &format!("{}", reference.gameid),
                                    &hash,
                                    &reference.gamerom).unwrap();
      rom           = reference.gamerom;
      name          = PathBuf::from(rom.clone()).file_stem().unwrap().to_str().unwrap().to_string();
      pkgname       = name.clone();
   }
   else {
      systemid = match matches.opt_str("s") {
         Some(x) => {
            x.parse::<u32>().unwrap()
         },
         None    => {
            print_usage(&program, opts);
            return;
         }
      };

      rom = match matches.opt_str("r") {
         Some(x) => {
            x.to_string()
         },
         None    => {
            print_usage(&program, opts);
            return;
         }
      };

      id  = match matches.opt_str("i") {
         Some(x) => {
            x.to_string()
         },
         None    => {
            "0".to_string()
         }
      };

      name = match matches.opt_str("n") {
         Some(x) => {
            x.to_string()
         },
         None    => {
            rom.clone()
         }
      };

      pkgname = match matches.opt_str("p") {
         Some(x) => {
            x.to_string()
         },
         None    => {
            rom.clone()
         }
      };

      system   = conf.system_find(systemid);
      hash = checksums::hash_file(Path::new(&rom), checksums::Algorithm::SHA1);
      pb.set_message(&format!("Fetching game infos"));
      jeuinfos = JeuInfos::get(&conf, &system, &id, &hash, &name).unwrap();
   }

   pb.println(format!("👌 Game informations"));
   pb.inc(1);

   pb.set_message(&format!("Downloading medias"));
   package  = Package::new(jeuinfos, &name, &rom, &hash).unwrap();
   package.set_pkgname(&pkgname);
   package.fetch().unwrap();

   pb.println(format!("👌 Downloaded medias"));
   pb.inc(1);

   pb.set_message(&format!("Writing PKGBUILD"));
   package.build(&system).unwrap();

   pb.println(format!("👌 PKGBUILD written"));
   pb.inc(1);
}
