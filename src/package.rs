use snafu::{ResultExt, Snafu};
use serde_xml_rs::to_string;
use std::{
   fs::File,
   io::Write,
   path::Path
};

use jeuinfos;
use jeuinfos::JeuInfos;
use conf::System;
use emulationstation::Game;

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
   pub medias: Medias
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
      let mut lines   = Vec::new();
      let     romname = self.name_normalize();

      lines.push(format!("pkgname=('{}{}')",
                         system.basename,
                         romname
                        ));
      lines.push(format!("_romname=\"{}\"", romname));
      lines.push("pkgver=1".to_string());
      lines.push("pkgrel=1".to_string());
      lines.push(format!("pkgdesc=\"{}\"", game.name));
      lines.push("arch=('any')".to_string());
      lines.push(format!("url=\"https://screenscraper.fr/gameinfos.php?gameid={}\"",
                 self.jeu.id));
      lines.push("license=('All rights reserved')".to_string());

      if let Some(ref x) = system.depends {
         lines.push(format!("depends=('{}')", x));
      }

      let sourcerom = self.rom.replace("'", "'\\''");

      lines.push(format!("source=('{}'", sourcerom));

      if let Some(ref _x) = self.medias.video {
         lines.push(        "        'video.mp4'".to_string());
      }

      if let Some(ref _x) = self.medias.bezel {
         lines.push(     "        'bezel.png'".to_string());
      }

      if let Some(ref _x) = self.medias.box3d {
         lines.push(     "        'box3d.png'".to_string());
      }

      if let Some(ref _x) = self.medias.thumbnail {
         lines.push(     "        'thumbnail.png'".to_string());
      }
      lines.push(        "        'description.xml')".to_string());

      lines.push("noextract=(\"${source[@]##*/}\")".to_string());

      lines.push(format!("sha1sums=('{}'", self.hash));

      if let Some(ref x) = self.medias.video {
         lines.push(format!("          '{}'", x.sha1));
      }

      if let Some(ref x) = self.medias.bezel {
         lines.push(format!("          '{}'", x.sha1));
      }

      if let Some(ref x) = self.medias.box3d {
         lines.push(format!("          '{}'", x.sha1));
      }

      if let Some(ref x) = self.medias.thumbnail {
         lines.push(format!("          '{}'", x.sha1));
      }

      lines.push(format!("          '{}'",
                         checksums::hash_file(Path::new("description.xml"), checksums::Algorithm::SHA1)
                        ));
      lines.push("         )".to_string());

      lines.push("build()\n{\n  true\n}\n".to_string());
      lines.push("package()\n{".to_string());

      lines.push(format!("   mkdir -m 0700 -p \"$pkgdir/roms/{}/data/$_romname/\"",
                         system.dir
                        )
                );

      lines.push(format!("   install -Dm600 \"{}\" \"$pkgdir\"/roms/{}/\"{}\"",
                         self.rom.replace("$", "\\$"),
                         system.dir,
                         self.rom.replace("$", "\\$")
                        )
                );

      if let Some(ref _x) = self.medias.video {
         lines.push(format!("   install -Dm600 {{,\"$pkgdir\"/roms/{}/data/$_romname/}}video.mp4",
                            system.dir
                           )
                   );
      }

      if let Some(ref _x) = self.medias.bezel {
         lines.push(format!("   install -Dm600 {{,\"$pkgdir\"/roms/{}/data/$_romname/}}bezel.png",
                            system.dir
                           )
                   );
      }

      if let Some(ref _x) = self.medias.box3d {
         lines.push(format!("   install -Dm600 {{,\"$pkgdir\"/roms/{}/data/$_romname/}}box3d.png",
                            system.dir
                           )
                   );
      }

      lines.push(format!("   install -Dm600 {{,\"$pkgdir\"/roms/{}/data/$_romname/}}thumbnail.png",
                         system.dir
                        )
                );
      lines.push(format!("   install -Dm600 {{,\"$pkgdir\"/roms/{}/data/$_romname/}}description.xml",
                         system.dir
                        )
                );
      lines.push("}".to_string());

      let mut f = File::create("./PKGBUILD").unwrap();

      for i in &lines {
         write!(f, "{}\n", i).unwrap();
      }

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