use snafu::{ResultExt, Snafu};
use serde_xml_rs::to_string;
use std::{
   fs::File,
   io::Write,
   path::Path,
   fmt
};

use jeuinfos;
use jeuinfos::JeuInfos;
use conf::System;
use emulationstation::Game;

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
   pub box3d:     Option<jeuinfos::Media>,
   pub thumbnail: Option<jeuinfos::Media>,
   pub bezel:     Option<jeuinfos::Media>,
   pub video:     Option<jeuinfos::Media>
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
                                 .to_lowercase()
   }

   pub fn new(mut jeu: JeuInfos, name: &String, file: &String, hash: &String) -> Result<Package> {
      let box3d     = jeu.media("box-3D");
      let thumbnail = jeu.media("mixrbv2");
      let video     = match jeu.media("video-normalized") {
         Some(x) => { Some(x) },
         None    => {
            jeu.media("video")
         }
      };
      let bezel     = jeu.media("bezel-16-9");

      Ok(Package {
         rom: file.to_string(),
         hash: hash.to_string(),
         jeu,
         name: name.to_string(),
         medias: Medias {
            box3d,
            thumbnail,
            bezel,
            video
         }
      })
   }

   pub fn fetch(&mut self) -> Result<()> {
      if let Some(ref mut x) = self.medias.box3d {
         x.download("./box3d.png").context(MediaDownload { filename: "box-3D".to_string() })?;
      }

      if let Some(ref mut x) = self.medias.thumbnail {
         x.download("./thumbnail.png").context(MediaDownload { filename: "mixrbv2".to_string() })?;
      }

      if let Some(ref mut x) = self.medias.bezel {
         x.download("./bezel.png").context(MediaDownload { filename: "bezel-16-9".to_string() })?;
      }

      if let Some(ref mut x) = self.medias.video {
         x.download("./video.mp4").context(MediaDownload { filename: "video-normalized".to_string() })?;
      }

      Ok(())
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
         pkgbuild.source.push("video.mp4".to_string());
         pkgbuild.sha1sums.push(x.sha1.clone());
      }

      if let Some(ref x) = self.medias.bezel {
         pkgbuild.source.push("bezel.png".to_string());
         pkgbuild.sha1sums.push(x.sha1.clone());
      }

      if let Some(ref x) = self.medias.box3d {
         pkgbuild.source.push("box3d.png".to_string());
         pkgbuild.sha1sums.push(x.sha1.clone());
      }

      if let Some(ref x) = self.medias.thumbnail {
         pkgbuild.source.push("thumbnail.png".to_string());
         pkgbuild.sha1sums.push(x.sha1.clone());
      }

      match system.id {
         20 => {
            pkgbuild.build.push("  IFS=$'\\n'".to_string());
            pkgbuild.build.push("  cuefile=$(ls *.cue)".to_string());
            pkgbuild.build.push("  sed -i \"s@FILE \\\"@FILE \\\"data/$_romname/@g\" ${cuefile}".to_string());

            pkgbuild.package.push("  IFS=$'\\n'".to_string());
            pkgbuild.package.push("  mkdir -m 0700 -p \"$pkgdir/roms/segacd/data/$_romname/\"".to_string());
            pkgbuild.package.push("  cuefile=$(ls *.cue)".to_string());
            pkgbuild.package.push("  install -Dm600 ${cuefile} \"$pkgdir\"/roms/segacd/${cuefile}".to_string());
            pkgbuild.package.push("  for file in $(ls *.bin); do".to_string());
            pkgbuild.package.push("    install -Dm600 {,\"$pkgdir\"/roms/segacd/data/$_romname/}${file}".to_string());
            pkgbuild.package.push("  done".to_string());

            pkgbuild.package.push(format!("  sed -i \"s|{}|$cuefile|\" description.xml", self.rom.replace("$", "\\$")));
            pkgbuild.package.push("  for file in $(ls *.mp4 *.png *.xml); do".to_string());
            pkgbuild.package.push(format!("    install -Dm600 {{,\"$pkgdir\"/roms/{}/data/$_romname/}}$file", system.dir));
            pkgbuild.package.push("  done".to_string());

         }
         _ => {
            pkgbuild.build.push("  true".to_string());

            pkgbuild.package.push(format!("  mkdir -m 0700 -p \"$pkgdir/roms/{}/data/$_romname/\"", system.dir));
            pkgbuild.package.push(format!(
               "  install -Dm600 \"{}\" \"$pkgdir\"/roms/{}/\"{}\"",
               self.rom.replace("$", "\\$"),
               system.dir,
               self.rom.replace("$", "\\$")
            ));
            pkgbuild.package.push("  for file in $(ls *.mp4 *.png *.xml); do".to_string());
            pkgbuild.package.push(format!("    install -Dm600 {{,\"$pkgdir\"/roms/{}/data/$_romname/}}$file", system.dir));
            pkgbuild.package.push("  done".to_string());
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

      if let Some(_x) = &self.medias.box3d {
         game.thumbnail = Some(format!("./data/{}/box3d.png", romname));
      }


      if let Some(_x) = &self.medias.video {
         game.video     = Some(format!("./data/{}/video.mp4", romname));
      }

      let s =  to_string(&game).unwrap();

      std::fs::write("./description.xml", &s)
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