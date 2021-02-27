use snafu::{ResultExt, Snafu};
use serde_xml_rs::to_string;
use std::{
   fs::File,
   io::Write,
   path::Path,
   fmt
};

use super::jeuinfos;
use super::jeuinfos::JeuInfos;
use super::conf::System;
use super::emulationstation::Game;

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
   pub image:         Option<jeuinfos::Media>,
   pub thumbnail:     Option<jeuinfos::Media>,
   pub bezel:         Option<jeuinfos::Media>,
   pub video:         Option<jeuinfos::Media>,
   pub marquee:       Option<jeuinfos::Media>,
   pub screenshot:    Option<jeuinfos::Media>,
   pub wheel:         Option<jeuinfos::Media>,
   pub manual:        Option<jeuinfos::Media>,
}

pub struct Package {
   pub rom:    String,
   pub hash:   String,
   pub jeu:    JeuInfos,
   pub name:   String,
   pub medias: Medias,
}

#[derive(Debug, Snafu)]
pub enum Error {
   #[snafu(display("Failed to find media {}", filename))]
   MediaFind {
      filename: String,
   },

   #[snafu(display("Failed to fetch media {}: {}", filename, source))]
   MediaDownload {
      filename: String,
      source: jeuinfos::Error
   },

   #[snafu(display("Failed to write {}: {}", filename, source))]
   IoError {
      filename: String,
      source: std::io::Error
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

   pub fn new(mut jeu: JeuInfos, name: &String, file: &String, hash: &String) -> Result<Package> {
      let thumbnail  = jeu.media("box-2D");
      let image      = jeu.media("sstitle");
      let video      = match jeu.media("video-normalized") {
         Some(x) => { Some(x) },
         None    => {
            jeu.media("video")
         }
      };
      let bezel      = jeu.media("bezel-16-9");
      let marquee    = jeu.media("marquee");
      let screenshot = jeu.media("ss");
      let wheel      = jeu.media("wheel");
      let manual     = jeu.media("manuel");

      Ok(Package {
         rom: file.to_string(),
         hash: hash.to_string(),
         jeu,
         name: name.to_string(),
         medias: Medias {
            image,
            thumbnail,
            bezel,
            video,
            marquee,
            screenshot,
            wheel,
            manual
         }
      })
   }

   pub fn set_pkgname(&mut self, name: &String) {
      self.name = name.clone();
   }

   pub fn build_pkgbuild(&mut self, system: &System, game: &Game) -> Result<()> {
      let     romname   = self.name_normalize();
      let     sourcerom = self.rom.replace("'", "'\\''");
      let mut pkgbuild  = Pkgbuild {
         pkgname:  format!("{}{}", system.basename, romname),
         romname:  romname.clone(),
         pkgver:   "1".to_string(),
         pkgrel:   1,
         pkgdesc:  game.name.clone(),
         url:      format!("https://screenscraper.fr/gameinfos.php?gameid={}", self.jeu.id),
         depends:  system.depends.clone(),
         source:   Vec::new(),
         sha1sums: Vec::new(),
         build:    Vec::new(),
         package:  Vec::new()
      };

      pkgbuild.source.push(sourcerom.clone());
      pkgbuild.sha1sums.push(self.hash.clone());

      pkgbuild.source.push("description.xml".to_string());
      pkgbuild.sha1sums.push(checksums::hash_file(Path::new("description.xml"), checksums::Algorithm::SHA1));

      if let Some(ref x) = self.medias.video {
         pkgbuild.source.push(format!(
         							  "video.mp4::https://screenscraper.fr/medias/{}/{}/video.mp4",
         							  system.id,
         							  self.jeu.id
         							 ));
         pkgbuild.sha1sums.push(x.sha1.clone());
      }

      if let Some(ref x) = self.medias.bezel {
         pkgbuild.source.push(format!("bezel.png::https://screenscraper.fr/medias/{}/{}/bezel-16-9({}).png",
                                      system.id,
                                      self.jeu.id,
                                      x.region.as_ref().unwrap_or(&"wor".to_string())
                                     ));
         pkgbuild.sha1sums.push(x.sha1.clone());
      }

      if let Some(ref x) = self.medias.image {
         pkgbuild.source.push(format!(
         							  "image.png::https://screenscraper.fr/medias/{}/{}/sstitle({}).png",
         							  system.id,
         							  self.jeu.id,
         							  x.region.as_ref().unwrap_or(&"wor".to_string())
         							 ));
         pkgbuild.sha1sums.push(x.sha1.clone());
      }

      if let Some(ref x) = self.medias.thumbnail {
         pkgbuild.source.push(format!("thumbnail.png::https://screenscraper.fr/medias/{}/{}/box-2D({}).png",
                  					  system.id,
                  					  self.jeu.id,
                  					  x.region.as_ref().unwrap_or(&"wor".to_string())
                  					 ));
         pkgbuild.sha1sums.push(x.sha1.clone());
      }

      if let Some(ref x) = self.medias.marquee {
         pkgbuild.source.push(format!("marquee.png::https://screenscraper.fr/medias/{}/{}/marquee.png",
                           			  system.id,
                           			  self.jeu.id
                           			 ));
         pkgbuild.sha1sums.push(x.sha1.clone());
      }

      if let Some(ref x) = self.medias.screenshot {
         pkgbuild.source.push(format!("screenshot.png::https://screenscraper.fr/medias/{}/{}/ss({}).png",
                           			  system.id,
                           			  self.jeu.id,
                           			  x.region.as_ref().unwrap_or(&"wor".to_string())
                           			 ));
         pkgbuild.sha1sums.push(x.sha1.clone());
      }

      if let Some(ref x) = self.medias.wheel {
         // ScreenScraper's region are often false concerning wheels.
         // It can reference an 'us' region while the URL links to 'wor', etc.
         // Very confusing.
		 let i           = x.url.find("media=").unwrap() + 6;
		 let (_, region) = x.url.split_at(i);

         pkgbuild.source.push(format!("wheel.png::https://screenscraper.fr/medias/{}/{}/{}.png",
                                 	  system.id,
                                 	  self.jeu.id,
                                 	  region
                                 	  ));
         pkgbuild.sha1sums.push(x.sha1.clone());
      }

      if let Some(ref x) = self.medias.manual {
         // ScreenScraper's region are often false concerning manuals.
         // It can reference an 'us' region while the URL links to 'wor', etc.
         // Very confusing.
		 let i           = x.url.find("media=").unwrap() + 6;
		 let (_, region) = x.url.split_at(i);

         pkgbuild.source.push(format!("manual.pdf::https://screenscraper.fr/medias/{}/{}/{}.pdf",
                                      system.id,
                                      self.jeu.id,
                                      region
         							  ));
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
            pkgbuild.package.push("  for file in $(ls *.mp4 *.png *.xml *.pdf); do".to_string());
            pkgbuild.package.push(format!("    install -Dm600 {{,\"$pkgdir\"/userdata/roms/{}/data/$_romname/}}$file", system.dir));
            pkgbuild.package.push("  done".to_string());

         },
         57  => {
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
            pkgbuild.package.push("  for file in $(ls *.mp4 *.png *.xml *.pdf); do".to_string());
            pkgbuild.package.push(format!("    install -Dm600 {{,\"$pkgdir\"/userdata/roms/{}/data/$_romname/}}$file", system.dir));
            pkgbuild.package.push("  done".to_string());
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
            pkgbuild.package.push("  for file in $(ls *.mp4 *.png *.xml *.pdf); do".to_string());
            pkgbuild.package.push(format!("    install -Dm600 {{,\"$pkgdir\"/userdata/roms/{}/data/$_romname/}}$file", system.dir));
            pkgbuild.package.push("  done".to_string());

            pkgbuild.package.push(format!("   echo \"gamelist = {}\" >  \"$pkgdir\"/userdata/system/pacman/batoexec/${{pkgname[0]}}", system.dir));
            pkgbuild.package.push("   cat description.xml          >> \"$pkgdir\"/userdata/system/pacman/batoexec/${pkgname[0]}".to_string());
         }
      };

      let mut f = File::create("./PKGBUILD").unwrap();
      write!(f, "{}", pkgbuild).unwrap();
      Ok(())
   }

   pub fn build(&mut self, system: &System) -> Result<()> {
      let     romname = self.name_normalize();
      let mut game    = Game::french(&self.jeu, &self.rom).unwrap();

      if let Some(_x) = &self.medias.thumbnail {
         game.image     = Some(format!("./data/{}/thumbnail.png", romname));
      }

      if let Some(_x) = &self.medias.image {
         game.thumbnail = Some(format!("./data/{}/image.png", romname));
      }


      if let Some(_x) = &self.medias.video {
         game.video     = Some(format!("./data/{}/video.mp4", romname));
      }

      if let Some(_x) = &self.medias.marquee {
         game.marquee   = Some(format!("./data/{}/marquee.png", romname));
      }

      if let Some(_x) = &self.medias.screenshot {
         game.screenshot= Some(format!("./data/{}/screenshot.png", romname));
      }

      if let Some(_x) = &self.medias.wheel {
         game.wheel     = Some(format!("./data/{}/wheel.png", romname));
      }

      if let Some(_x) = &self.medias.manual {
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
                     .context(IoError { filename: "./launcher".to_string() })?;

            game.path = format!("./{}.sh", game.name);
         },
         57 => {
            game.path = format!("./{}.m3u", romname);
         },
         _ => { }
      }

      let s =  to_string(&game).unwrap();

      std::fs::write("./description.xml", &s.replace("Game>", "game>"))
         .context(IoError { filename: "./description.xml".to_string() })?;

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
