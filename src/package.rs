use serde_derive::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use serde_xml_rs::to_string;
use std::{
   fs::{File,create_dir_all},
   io::Write,
   path::Path,
   fmt
};

use super::conf::System;
use super::emulationstation::Game;

use screenscraper::jeuinfo::{JeuInfo,Media};

pub struct Pkgbuild {
   pub pkgname:  String,
   pub romname:  String,
   pub pkgver:   String,
   pub pkgrel:   u32,
   pub pkgdesc:  String,
   pub url:      String,
   pub depends:  Option<String>,
   pub source:   Vec<String>,
   pub sha1sums: Vec<String>,
   pub build:    Vec<String>,
   pub package:  Vec<String>
}

pub struct Medias {
   pub image:         Option<Media>,
   pub thumbnail:     Option<Media>,
   pub bezel:         Option<Media>,
   pub video:         Option<Media>,
   pub marquee:       Option<Media>,
   pub screenshot:    Option<Media>,
   pub wheel:         Option<Media>,
   pub manual:        Option<Media>,
}

pub struct Package {
   pub rom:     String,
   pub rom_url: String,
   pub hash:    String,
   pub jeu:     Option<JeuInfo>,
   pub name:    String,
   pub medias:  Medias,
}

#[derive(Debug, Snafu)]
pub enum Error {
   #[snafu(display("Failed to write {}: {}", filename, source))]
   WriteResult {
      source: std::io::Error,
      filename: String,
   },
}

type Result<T, E = Error> = std::result::Result<T, E>;

impl Package {
  pub fn name_normalize(&self) -> String {
    self.name.replace("(", "")
             .replace(")", "")
             .replace(" ", "")
             .replace(",", "")
             .replace("'", "")
             .replace("!", "")
             .replace("&", "and")
             .replace("%", "")
             .replace("^", "")
             .replace(";", "")
             .replace("$", "")
             .replace("~", "-")
             .replace("=", "-")
             .replace("[", "")
             .replace("]", "")
             .to_lowercase()
  }

  pub fn new(mut jeu: Option<JeuInfo>, file: &String, url: &String, hash: &String) -> Result<Package> {
    let medias = match jeu {
      Some(ref mut x) => {
        Medias {
          image: x.media("sstitle"),
          thumbnail: x.media("box-2D"),
          bezel: x.media("bezel-16-9"),
          video: match x.media("video-normalized") {
            Some(x) => Some(x),
            None    => x.media("video")
          },
          marquee: x.media("marquee"),
          screenshot: x.media("ss"),
          wheel: x.media("wheel"),
          manual: x.media("manuel"),
        }
      },
      None => {
        Medias {
          image: None,
          thumbnail: None,
          bezel: None,
          video: None,
          marquee: None,
          screenshot: None,
          wheel: None,
          manual: None
        }
      }
    };
    Ok(Package {
      rom: file.to_string(),
      rom_url: url.to_string(),
      hash: hash.to_string(),
      jeu: jeu.clone(),
      name: file.to_string(),
      medias
    })
  }

   pub fn set_pkgname(&mut self, name: &String) {
      self.name = name.clone();
   }

  pub fn build_pkgbuild(&mut self, system: &System, game: &Game) -> Result<()> {
    let     romname   = self.name_normalize();
    let     sourcerom = self.rom.replace("'", "'\\''");
    let directory = Path::new(&self.rom).with_extension("");
    let mut pkgbuild  = Pkgbuild {
      pkgname:  format!("{}{}", system.basename, romname.clone()),
      romname:  romname.clone(),
      pkgver:   "1".to_string(),
      pkgrel:   1,
      pkgdesc:  game.name.clone(),
      url:      match &self.jeu {
        Some(x) => format!("https://screenscraper.fr/gameinfos.php?gameid={}", x.id),
        None    => format!(""),
      },
      depends:  system.depends.clone(),
      source:   Vec::new(),
      sha1sums: Vec::new(),
      build:    Vec::new(),
      package:  Vec::new()
    };

    pkgbuild.source.push(format!("{}::{}", sourcerom, self.rom_url));
    pkgbuild.sha1sums.push(self.hash.clone());

    pkgbuild.source.push("description.xml".to_string());
    pkgbuild.sha1sums.push(checksums::hash_file(Path::new(&format!("{}/description.xml", directory.display())), checksums::Algorithm::SHA1));

    if let Some(ref x) = self.medias.video {
      pkgbuild.source.push(format!("video.mp4::https://screenscraper.fr/medias/{}/{}/video.mp4", system.id, self.jeu.as_ref().unwrap().id));
      pkgbuild.sha1sums.push(x.sha1.clone());
    }

    if let Some(ref x) = self.medias.bezel {
      pkgbuild.source.push(format!("bezel.png::https://screenscraper.fr/medias/{}/{}/bezel-16-9({}).{}", system.id, self.jeu.as_ref().unwrap().id, x.region.as_ref().unwrap_or(&"wor".to_string()), x.format));
      pkgbuild.sha1sums.push(x.sha1.clone());
    }

    if let Some(ref x) = self.medias.image {
      // ScreenScraper's region are often false concerning wheels.
      // It can reference an 'us' region while the URL links to 'wor', etc.
      // Very confusing.
      let i           = x.url.find("media=").unwrap() + 6;
      let (_, region) = x.url.split_at(i);
      pkgbuild.source.push(format!("image.png::https://screenscraper.fr/medias/{}/{}/{}.{}", system.id, self.jeu.as_ref().unwrap().id, region, x.format));
      pkgbuild.sha1sums.push(x.sha1.clone());
    }

    if let Some(ref x) = self.medias.thumbnail {
      // ScreenScraper's region are often false concerning wheels.
      // It can reference an 'us' region while the URL links to 'wor', etc.
      // Very confusing.
      let i           = x.url.find("media=").unwrap() + 6;
      let (_, region) = x.url.split_at(i);
      pkgbuild.source.push(format!("thumbnail.png::https://screenscraper.fr/medias/{}/{}/{}.png", system.id, self.jeu.as_ref().unwrap().id, region));
      pkgbuild.sha1sums.push(x.sha1.clone());
    }

    if let Some(ref x) = self.medias.marquee {
      pkgbuild.source.push(format!("marquee.png::https://screenscraper.fr/medias/{}/{}/marquee.{}", system.id, self.jeu.as_ref().unwrap().id, x.format));
      pkgbuild.sha1sums.push(x.sha1.clone());
    }

    if let Some(ref x) = self.medias.screenshot {
      pkgbuild.source.push(format!("screenshot.png::https://screenscraper.fr/medias/{}/{}/ss({}).{}",system.id,self.jeu.as_ref().unwrap().id,x.region.as_ref().unwrap_or(&"wor".to_string()), x.format));
      pkgbuild.sha1sums.push(x.sha1.clone());
    }

    if let Some(ref x) = self.medias.wheel {
      // ScreenScraper's region are often false concerning wheels.
      // It can reference an 'us' region while the URL links to 'wor', etc.
      // Very confusing.
      let i           = x.url.find("media=").unwrap() + 6;
      let (_, region) = x.url.split_at(i);

      pkgbuild.source.push(format!("wheel.png::https://screenscraper.fr/medias/{}/{}/{}.{}", system.id, self.jeu.as_ref().unwrap().id, region, x.format));
      pkgbuild.sha1sums.push(x.sha1.clone());
    }

    if let Some(ref x) = self.medias.manual {
      // ScreenScraper's region are often false concerning manuals.
      // It can reference an 'us' region while the URL links to 'wor', etc.
      // Very confusing.
      let i           = x.url.find("media=").unwrap() + 6;
      let (_, region) = x.url.split_at(i);

      pkgbuild.source.push(format!("manual.pdf::https://screenscraper.fr/medias/{}/{}/{}.pdf", system.id, self.jeu.as_ref().unwrap().id, region));
      pkgbuild.sha1sums.push(x.sha1.clone());
    }

      match system.id {
         20  => {
            pkgbuild.build.push("  IFS=$'\\n'".to_string());
            pkgbuild.build.push("  cuefile=$(ls *.cue)".to_string());
            pkgbuild.build.push("  sed -i \"s@FILE \\\"@FILE \\\"data/$_romname/@g\" ${cuefile}".to_string());

            pkgbuild.package.push("  IFS=$'\\n'".to_string());
            pkgbuild.package.push("  mkdir -m 0700 -p \"$pkgdir/userdata/roms/segacd/data/$_romname/\"".to_string());
            pkgbuild.package.push("  cuefile=$(ls *.cue)".to_string());
            pkgbuild.package.push("  install -Dm600 ${cuefile} \"$pkgdir\"/userdata/roms/segacd/${cuefile}".to_string());
            pkgbuild.package.push("  for file in $(ls *.bin); do".to_string());
            pkgbuild.package.push("    install -Dm600 {,\"$pkgdir\"/userdata/roms/segacd/data/$_romname/}${file}".to_string());
            pkgbuild.package.push("  done".to_string());

            pkgbuild.package.push(format!("  sed -i \"s|{}|$cuefile|\" description.xml", self.rom.replace("$", "\\$")));
            pkgbuild.package.push("  for file in $(ls *.mp4 *.png *.xml *.pdf, *.jpg); do".to_string());
            pkgbuild.package.push(format!("    install -Dm600 {{,\"$pkgdir\"/userdata/roms/{}/data/$_romname/}}$file", system.dir));
            pkgbuild.package.push("  done".to_string());

         },
         22 | 57  => {
            pkgbuild.build.push(
               "  IFS=$'\\n'".to_string()
            );
            pkgbuild.build.push(
               "  for file in $(ls *.chd); do".to_string()
            );

            pkgbuild.build.push(
               "    echo \".data/$_romname/${file}\" >>${_romname}.m3u".to_string()
            );

            pkgbuild.build.push(
               "  done".to_string()
            );

            pkgbuild.package.push(format!("  mkdir -m 0700 -p \"$pkgdir/userdata/roms/{}/data/$_romname/\" \\", system.dir));
            pkgbuild.package.push("                   \"$pkgdir/userdata/system/pacman/batoexec/\"".to_string());

            pkgbuild.package.push(format!("  mkdir -p 0700 -p \"$pkgdir/userdata/roms/{}/data/$_romname/\" \"$pkgdir/userdata/roms/{}/.data/$_romname\"",
                                          system.dir,
                                          system.dir
                                         )
            );

            pkgbuild.package.push(
               format!("  install -m 0600 *.chd \"$pkgdir/userdata/roms/{}/.data/$_romname/\"", system.dir)
            );
            pkgbuild.package.push(
               format!("  install -m 0600 \"${{_romname}}.m3u\" \"$pkgdir/userdata/roms/{}/\"", system.dir)
            );
            pkgbuild.package.push("  for file in $(ls *.mp4 *.png *.xml *.pdf, *.jpg); do".to_string());
            pkgbuild.package.push(format!("    install -Dm600 {{,\"$pkgdir\"/userdata/roms/{}/data/$_romname/}}$file", system.dir));
            pkgbuild.package.push("  done".to_string());

            pkgbuild.package.push(format!("   echo \"gamelist = {}\" >  \"$pkgdir\"/userdata/system/pacman/batoexec/${{pkgname[0]}}", system.dir));
            pkgbuild.package.push("   cat description.xml          >> \"$pkgdir\"/userdata/system/pacman/batoexec/${pkgname[0]}".to_string());
         }
         _ => {
            pkgbuild.build.push("  true".to_string());

            pkgbuild.package.push(format!("  mkdir -m 0700 -p \"$pkgdir/userdata/roms/{}/data/$_romname/\" \\", system.dir));
            pkgbuild.package.push("                   \"$pkgdir/userdata/system/pacman/batoexec/\"".to_string());
            pkgbuild.package.push(format!(
               "  install -Dm600 \"{}\" \"$pkgdir\"/userdata/roms/{}/\"{}\"",
               self.rom.replace("$", "\\$"),
               system.dir,
               self.rom.replace("$", "\\$")
            ));
            pkgbuild.package.push("  for file in $(ls *.mp4 *.png *.jpg *.xml *.pdf); do".to_string());
            pkgbuild.package.push(format!("    install -Dm600 {{,\"$pkgdir\"/userdata/roms/{}/data/$_romname/}}$file", system.dir));
            pkgbuild.package.push("  done".to_string());

            pkgbuild.package.push(format!("   echo \"gamelist = {}\" >  \"$pkgdir\"/userdata/system/pacman/batoexec/${{pkgname[0]}}", system.dir));
            pkgbuild.package.push("   cat description.xml          >> \"$pkgdir\"/userdata/system/pacman/batoexec/${pkgname[0]}".to_string());
         }
      };

      let file = format!("{}/PKGBUILD", directory.display());
      println!("Writing {}", file);

      let mut f = File::create(file).unwrap();
      write!(f, "{}", pkgbuild).unwrap();
      Ok(())
   }

   pub fn build(&mut self, system: &System) -> Result<()> {
      let     romname = self.name_normalize();
      let mut game    = Game::french(&self.jeu, &self.rom).unwrap();

      if let Some(x) = &self.medias.thumbnail {
         game.image     = Some(format!("./data/{}/thumbnail.{}", romname, x.format));
      }

      if let Some(x) = &self.medias.image {
         game.thumbnail = Some(format!("./data/{}/image.{}", romname, x.format));
      }


      if let Some(_x) = &self.medias.video {
         game.video     = Some(format!("./data/{}/video.mp4", romname));
      }

      if let Some(x) = &self.medias.marquee {
         game.marquee   = Some(format!("./data/{}/marquee.{}", romname, x.format));
      }

      if let Some(x) = &self.medias.screenshot {
         game.screenshot= Some(format!("./data/{}/screenshot.{}", romname, x.format));
      }

      if let Some(x) = &self.medias.wheel {
         game.wheel     = Some(format!("./data/{}/wheel.{}", romname, x.format));
      }

      if let Some(x) = &self.medias.manual {
         game.manual     = Some(format!("./data/{}/manual.pdf", romname));
      }

      match system.id {
         214 => {
            // Create launcher
            let mut s = String::new();

            s.push_str("DIR=\"$(dirname \"$(readlink -f \"$0\")\")\"\n");
            s.push_str("cd ${DIR}/.data/\n\n");
            s.push_str("export LD_LIBRARY_PATH=\"${DIR}/.data/lib/\"\n");
            s.push_str(&format!("./OpenBOR '{}'",
                                self.rom.replace("'", "'\\''")
                               )
                      );
            std::fs::write("./launcher", &s)
                     .context(WriteResultSnafu { filename: "./launcher".to_string() })?;

            game.path = format!("./{}.sh", game.name);
         },
         22 | 57 => {
            game.path = format!("./{}.m3u", romname);
         },
         _ => { }
      }

      let directory = Path::new(&self.rom).with_extension("");

      create_dir_all(&directory);

      let s =  to_string(&game).unwrap();
      let file = format!("{}/description.xml", directory.display());

      println!("Writing {}", file);
      std::fs::write(file, &s.replace("Game>", "game>"))
         .context(WriteResultSnafu { filename: "./description.xml".to_string() })?;

      self.build_pkgbuild(system, &game).unwrap();
      Ok(())
   }
}

impl fmt::Display for Pkgbuild {
   fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
      let mut s = String::new();

      s.push_str(&format!("pkgname=('{}')\n", self.pkgname));
      s.push_str(&format!("_romname=\"{}\"\n", self.romname));
      s.push_str(&format!("pkgver={}\n", self.pkgver));
      s.push_str(&format!("pkgrel={}\n", self.pkgrel));
      s.push_str(&format!("pkgdesc=\"{}\"\n", self.pkgdesc));
      s.push_str("arch=('any')\n");
      s.push_str(&format!("url=\"{}\"\n", self.url));
      s.push_str("license=('All rights reserved')\n");

      if let Some(x) = &self.depends {
         s.push_str(&format!("depends=('{}')\n", x));
      }

      s.push_str("source=(\n");
      for item in &self.source {
         s.push_str(&format!("  '{}'\n", item));
      }
      s.push_str(")\n");


      s.push_str("sha1sums=(\n");
      for item in &self.sha1sums {
         s.push_str(&format!("  '{}'\n", item));
      }
      s.push_str(")\n");

      s.push_str("build()\n{\n");
      for line in &self.build {
         s.push_str(&format!("{}\n", line));
      }
      s.push_str("}\n");

      s.push_str("package()\n{\n");
      for line in &self.package {
         s.push_str(&format!("{}\n", line));
      }
      s.push_str("}\n");
      write!(f, "{}", s)
   }
}
