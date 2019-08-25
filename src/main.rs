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

use jeuinfos::JeuInfos;
use package::Package;
use conf::{
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
   let     name;
   let     confdir;
   let mut package;
   let     system;
   let     jeuinfos;
   let     hash;

   confdir = match dirs::config_dir() {
      Some(x) => { x },
      None    => {
         eprintln!("Failed to find user configuration dir");
         return;
      }
   };


   let     conf     = Conf::load(&format!("{}/rompom.yml", confdir.display())).unwrap();

   opts.optopt ("s", "system", "System to search for",     "SYSTEM");
   opts.optopt ("r", "rom",    "Rom file to package",      "ROM"   );
   opts.optopt ("i", "id",     "Game ID on Screenscraper", "ID"    );
   opts.optopt ("n", "name",   "Game name",                "NAME"  );
   opts.optflag("h", "help",   "print this help menu"              );
   opts.optflag("u", "update", "Update package"                    );

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
      hash          = checksums::hash_file(Path::new(&reference.gamerom),
                                           checksums::Algorithm::SHA1);
      system        = conf.system_find(reference.systemid);
      jeuinfos      = JeuInfos::get(&conf,
                                    &reference.systemid,
                                    &format!("{}", reference.gameid),
                                    &hash,
                                    &reference.gamerom).unwrap();
      rom = reference.gamerom;
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

      hash = checksums::hash_file(Path::new(&rom), checksums::Algorithm::SHA1);
      system   = conf.system_find(systemid);
      jeuinfos = JeuInfos::get(&conf, &system.id, &id, &hash, &rom).unwrap();
   }

   name = PathBuf::from(rom.clone()).file_stem().unwrap().to_str().unwrap().to_string();

   package  = Package::new(jeuinfos, &name, &rom, &hash).unwrap();
   package.fetch().unwrap();
   package.build(&system).unwrap();
}
