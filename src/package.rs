use minijinja::{context, Environment};
use serde_xml_rs::to_string;
use snafu::{ResultExt, Snafu};
use std::{fs::create_dir_all, path::Path};

use super::conf::System;
use super::emulationstation::Game;
use super::pkgbuild::Pkgbuild;

use screenscraper::jeuinfo::{JeuInfo, Media};

#[derive(Default)]
pub struct Medias {
  pub image: Option<Media>,
  pub thumbnail: Option<Media>,
  pub bezel: Option<Media>,
  pub video: Option<Media>,
  pub marquee: Option<Media>,
  pub screenshot: Option<Media>,
  pub wheel: Option<Media>,
  pub manual: Option<Media>,
}

pub struct Package {
  pub rom: String,
  pub rom_url: String,
  pub hash: String,
  pub jeu: Option<JeuInfo>,
  pub name: String,
  pub medias: Medias,
}

#[derive(Debug, Snafu)]
pub enum Error {
  #[snafu(display("Failed to write {}: {}", filename, source))]
  WriteResult {
    source: std::io::Error,
    filename: String,
  },
  #[snafu(display("Failed to write PKGBUILD: {}", source))]
  WritePkgbuild { source: super::pkgbuild::Error },
}

type Result<T, E = Error> = std::result::Result<T, E>;

fn media_region(url: &str) -> &str {
  url.find("media=").map(|i| &url[i + 6..]).unwrap_or("")
}

fn render_template(src: &str, ctx: &minijinja::Value) -> String {
  let mut env = Environment::new();
  env.add_template("t", src).unwrap();
  env.get_template("t").unwrap().render(ctx).unwrap()
}

impl Package {
  pub fn name_normalize(&self) -> String {
    self
      .name
      .replace("(", "")
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

  pub fn new(mut jeu: Option<JeuInfo>, file: &str, url: &str, hash: &str) -> Result<Package> {
    let medias = match jeu {
      Some(ref mut x) => Medias {
        image: x.media("sstitle"),
        thumbnail: x.media("box-2D"),
        bezel: x.media("bezel-16-9"),
        video: x.media("video-normalized").or_else(|| x.media("video")),
        marquee: x.media("marquee"),
        screenshot: x.media("ss"),
        wheel: x.media("wheel"),
        manual: x.media("manuel"),
      },
      None => Medias::default(),
    };
    Ok(Package {
      rom: file.to_string(),
      rom_url: url.to_string(),
      hash: hash.to_string(),
      jeu,
      name: file.to_string(),
      medias,
    })
  }

  pub fn build_pkgbuild(&mut self, system: &System, game: &Game) -> Result<()> {
    let romname = self.name_normalize();
    let sourcerom = self.rom.replace("'", "'\\''");
    let rom_escaped = self.rom.replace("$", "\\$");
    let directory = Path::new(&self.rom).with_extension("");
    let jeu_id = self.jeu.as_ref().map(|j| j.id.as_str()).unwrap_or("");

    let mut pkgbuild = Pkgbuild {
      pkgname: format!("{}{}", system.basename, romname),
      romname: romname.clone(),
      pkgver: "1".to_string(),
      pkgrel: 1,
      pkgdesc: game.name.clone(),
      url: if jeu_id.is_empty() {
        String::new()
      } else {
        format!("https://screenscraper.fr/gameinfos.php?gameid={}", jeu_id)
      },
      depends: system.depends.clone(),
      source: Vec::new(),
      sha1sums: Vec::new(),
      build: Vec::new(),
      package: Vec::new(),
    };

    pkgbuild.add_source(
      format!("{}::{}", sourcerom, self.rom_url),
      self.hash.clone(),
    );
    pkgbuild.add_source(
      "description.xml".to_string(),
      checksums::hash_file(
        Path::new(&format!("{}/description.xml", directory.display())),
        checksums::Algorithm::SHA1,
      ),
    );

    if let Some(ref x) = self.medias.video {
      pkgbuild.add_source(
        format!(
          "video.mp4::https://screenscraper.fr/medias/{}/{}/video.mp4",
          system.id, jeu_id
        ),
        x.sha1.clone(),
      );
    }

    if let Some(ref x) = self.medias.bezel {
      pkgbuild.add_source(
        format!(
          "bezel.png::https://screenscraper.fr/medias/{}/{}/bezel-16-9({}).{}",
          system.id,
          jeu_id,
          x.region.as_deref().unwrap_or("wor"),
          x.format
        ),
        x.sha1.clone(),
      );
    }

    if let Some(ref x) = self.medias.image {
      pkgbuild.add_source(
        format!(
          "image.png::https://screenscraper.fr/medias/{}/{}/{}.{}",
          system.id,
          jeu_id,
          media_region(&x.url),
          x.format
        ),
        x.sha1.clone(),
      );
    }

    if let Some(ref x) = self.medias.thumbnail {
      pkgbuild.add_source(
        format!(
          "thumbnail.png::https://screenscraper.fr/medias/{}/{}/{}.png",
          system.id,
          jeu_id,
          media_region(&x.url)
        ),
        x.sha1.clone(),
      );
    }

    if let Some(ref x) = self.medias.marquee {
      pkgbuild.add_source(
        format!(
          "marquee.png::https://screenscraper.fr/medias/{}/{}/marquee.{}",
          system.id, jeu_id, x.format
        ),
        x.sha1.clone(),
      );
    }

    if let Some(ref x) = self.medias.screenshot {
      pkgbuild.add_source(
        format!(
          "screenshot.png::https://screenscraper.fr/medias/{}/{}/ss({}).{}",
          system.id,
          jeu_id,
          x.region.as_deref().unwrap_or("wor"),
          x.format
        ),
        x.sha1.clone(),
      );
    }

    if let Some(ref x) = self.medias.wheel {
      pkgbuild.add_source(
        format!(
          "wheel.png::https://screenscraper.fr/medias/{}/{}/{}.{}",
          system.id,
          jeu_id,
          media_region(&x.url),
          x.format
        ),
        x.sha1.clone(),
      );
    }

    if let Some(ref x) = self.medias.manual {
      pkgbuild.add_source(
        format!(
          "manual.pdf::https://screenscraper.fr/medias/{}/{}/{}.pdf",
          system.id,
          jeu_id,
          media_region(&x.url)
        ),
        x.sha1.clone(),
      );
    }

    let ctx = context! { dir => system.dir, rom => rom_escaped };
    let (build_src, package_src) = match system.id {
      20 => (
        include_str!("../assets/templates/pkgbuild/segacd-build.jinja"),
        include_str!("../assets/templates/pkgbuild/segacd-package.jinja"),
      ),
      22 => (
        include_str!("../assets/templates/pkgbuild/psx-build.jinja"),
        include_str!("../assets/templates/pkgbuild/psx-package.jinja"),
      ),
      57 => (
        include_str!("../assets/templates/pkgbuild/ps2-build.jinja"),
        include_str!("../assets/templates/pkgbuild/ps2-package.jinja"),
      ),
      _ => (
        include_str!("../assets/templates/pkgbuild/default-build.jinja"),
        include_str!("../assets/templates/pkgbuild/default-package.jinja"),
      ),
    };
    pkgbuild.push_build_block(&render_template(build_src, &ctx));
    pkgbuild.push_package_block(&render_template(package_src, &ctx));

    pkgbuild.write(&directory).context(WritePkgbuildSnafu)
  }

  pub fn build(&mut self, system: &System, lang: &[&str]) -> Result<()> {
    let romname = self.name_normalize();
    let mut game = Game::from_jeuinfo(&self.jeu, &self.rom, lang);

    if let Some(x) = &self.medias.thumbnail {
      game.image = Some(format!("./data/{}/thumbnail.{}", romname, x.format));
    }
    if let Some(x) = &self.medias.image {
      game.thumbnail = Some(format!("./data/{}/image.{}", romname, x.format));
    }
    if self.medias.video.is_some() {
      game.video = Some(format!("./data/{}/video.mp4", romname));
    }
    if let Some(x) = &self.medias.marquee {
      game.marquee = Some(format!("./data/{}/marquee.{}", romname, x.format));
    }
    if let Some(x) = &self.medias.screenshot {
      game.screenshot = Some(format!("./data/{}/screenshot.{}", romname, x.format));
    }
    if let Some(x) = &self.medias.wheel {
      game.wheel = Some(format!("./data/{}/wheel.{}", romname, x.format));
    }
    if self.medias.manual.is_some() {
      game.manual = Some(format!("./data/{}/manual.pdf", romname));
    }

    match system.id {
      214 => {
        let mut s = String::new();
        s.push_str("DIR=\"$(dirname \"$(readlink -f \"$0\")\")\"\n");
        s.push_str("cd ${DIR}/.data/\n\n");
        s.push_str("export LD_LIBRARY_PATH=\"${DIR}/.data/lib/\"\n");
        s.push_str(&format!("./OpenBOR '{}'", self.rom.replace("'", "'\\''")));
        std::fs::write("./launcher", &s).context(WriteResultSnafu {
          filename: "./launcher".to_string(),
        })?;
        game.path = format!("./{}.sh", game.name);
      }
      22 | 57 => {
        game.path = format!("./{}.m3u", romname);
      }
      _ => {}
    }

    let directory = Path::new(&self.rom).with_extension("");
    create_dir_all(&directory).ok();

    let s = to_string(&game).unwrap();
    let file = format!("{}/description.xml", directory.display());
    std::fs::write(file, s.replace("Game>", "game>")).context(WriteResultSnafu {
      filename: "./description.xml".to_string(),
    })?;

    self.build_pkgbuild(system, &game)?;
    Ok(())
  }
}
