use minijinja::{context, Environment};
use serde::Serialize;
use snafu::{ResultExt, Snafu};
use std::{fs::create_dir_all, path::Path};

use super::conf::System;
use super::emulationstation::Game;
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
}

type Result<T, E = Error> = std::result::Result<T, E>;

fn media_region(url: &str) -> &str {
  url.find("media=").map(|i| &url[i + 6..]).unwrap_or("")
}

fn render_template(src: &str, ctx: &minijinja::Value) -> String {
  let mut env = Environment::new();
  env.set_trim_blocks(true);
  env.set_lstrip_blocks(true);
  env.add_template("t", src).unwrap();
  env.get_template("t").unwrap().render(ctx).unwrap()
}

fn generate_description_xml(game: &Game) -> String {
  let mut xml = String::new();
  let mut ser = quick_xml::se::Serializer::new(&mut xml);
  ser.indent(' ', 2);
  game.serialize(ser).unwrap();
  xml
}

/// Sets `game.path` to the system-specific value without performing any I/O.
fn apply_game_path(system: &System, game: &mut Game, romname: &str) {
  match system.id {
    214 => game.path = format!("./{}.sh", game.name),
    22 | 57 => game.path = format!("./{}.m3u", romname),
    _ => {}
  }
}

/// Lit le `pkgver` depuis un PKGBUILD existant. Retourne 0 si le fichier
/// n'existe pas ou ne contient pas de `pkgver=N` valide.
/// L'appelant incrémente de 1 pour obtenir le prochain pkgver.
pub fn read_pkgver(directory: &Path) -> u32 {
  let path = directory.join("PKGBUILD");
  std::fs::read_to_string(&path)
    .unwrap_or_default()
    .lines()
    .find_map(|line| {
      line
        .strip_prefix("pkgver=")
        .and_then(|v| v.trim().parse::<u32>().ok())
    })
    .unwrap_or(0)
}

impl Package {
  pub fn normalize_name(&self) -> String {
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

  /// Writes description.xml only when the content has changed.
  /// Returns `true` if the file was written (new or updated), `false` if unchanged.
  fn write_description_xml(&self, game: &Game, directory: &Path) -> Result<bool> {
    let xml = generate_description_xml(game);
    let path = format!("{}/description.xml", directory.display());
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing == xml {
      return Ok(false);
    }
    std::fs::write(&path, &xml).context(WriteResultSnafu { filename: path })?;
    Ok(true)
  }

  fn write_launcher(&self, system: &System, game: &mut Game, romname: &str) -> Result<()> {
    if system.id == 214 {
      let ctx = context! {
        rom => self.rom.replace("'", "'\\''"),
      };
      let launcher = render_template(
        include_str!("../assets/templates/launcher/openbor.jinja"),
        &ctx,
      );
      std::fs::write("./launcher", launcher).context(WriteResultSnafu {
        filename: "./launcher".to_string(),
      })?;
    }
    apply_game_path(system, game, romname);
    Ok(())
  }

  pub fn build_pkgbuild(&mut self, system: &System, game: &Game, pkgver: u32) -> Result<()> {
    let romname = self.normalize_name();
    let sourcerom = self.rom.replace("'", "'\\''");
    let rom_escaped = self.rom.replace("$", "\\$");
    let directory = Path::new(&self.rom).with_extension("");
    let jeu_id = self.jeu.as_ref().map(|j| j.id.as_str()).unwrap_or("");

    // Sources & checksums
    let mut sources: Vec<String> = Vec::new();
    let mut sha1sums: Vec<String> = Vec::new();

    sources.push(format!("{}::{}", sourcerom, self.rom_url));
    sha1sums.push(self.hash.clone());

    sources.push("description.xml".to_string());
    sha1sums.push(checksums::hash_file(
      Path::new(&format!("{}/description.xml", directory.display())),
      checksums::Algorithm::SHA1,
    ));

    if let Some(ref x) = self.medias.video {
      sources.push(format!(
        "video.mp4::https://screenscraper.fr/medias/{}/{}/video.mp4",
        system.id, jeu_id
      ));
      sha1sums.push(x.sha1.clone());
    }
    if let Some(ref x) = self.medias.bezel {
      sources.push(format!(
        "bezel.png::https://screenscraper.fr/medias/{}/{}/bezel-16-9({}).{}",
        system.id,
        jeu_id,
        x.region.as_deref().unwrap_or("wor"),
        x.format
      ));
      sha1sums.push(x.sha1.clone());
    }
    if let Some(ref x) = self.medias.image {
      sources.push(format!(
        "image.png::https://screenscraper.fr/medias/{}/{}/{}.{}",
        system.id,
        jeu_id,
        media_region(&x.url),
        x.format
      ));
      sha1sums.push(x.sha1.clone());
    }
    if let Some(ref x) = self.medias.thumbnail {
      sources.push(format!(
        "thumbnail.png::https://screenscraper.fr/medias/{}/{}/{}.png",
        system.id,
        jeu_id,
        media_region(&x.url)
      ));
      sha1sums.push(x.sha1.clone());
    }
    if let Some(ref x) = self.medias.marquee {
      sources.push(format!(
        "marquee.png::https://screenscraper.fr/medias/{}/{}/marquee.{}",
        system.id, jeu_id, x.format
      ));
      sha1sums.push(x.sha1.clone());
    }
    if let Some(ref x) = self.medias.screenshot {
      sources.push(format!(
        "screenshot.png::https://screenscraper.fr/medias/{}/{}/ss({}).{}",
        system.id,
        jeu_id,
        x.region.as_deref().unwrap_or("wor"),
        x.format
      ));
      sha1sums.push(x.sha1.clone());
    }
    if let Some(ref x) = self.medias.wheel {
      sources.push(format!(
        "wheel.png::https://screenscraper.fr/medias/{}/{}/{}.{}",
        system.id,
        jeu_id,
        media_region(&x.url),
        x.format
      ));
      sha1sums.push(x.sha1.clone());
    }
    if let Some(ref x) = self.medias.manual {
      sources.push(format!(
        "manual.pdf::https://screenscraper.fr/medias/{}/{}/{}.pdf",
        system.id,
        jeu_id,
        media_region(&x.url)
      ));
      sha1sums.push(x.sha1.clone());
    }

    // System-specific build/package sections
    let sys_ctx = context! { dir => system.dir, rom => rom_escaped };
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
    let build_section = render_template(build_src, &sys_ctx);
    let package_section = render_template(package_src, &sys_ctx);

    // Main PKGBUILD
    let url = if jeu_id.is_empty() {
      String::new()
    } else {
      format!("https://screenscraper.fr/gameinfos.php?gameid={}", jeu_id)
    };
    let ctx = context! {
      pkgname => format!("{}{}", system.basename, romname),
      romname => romname,
      pkgver => pkgver,
      pkgrel => 1_u32,
      pkgdesc => &game.name,
      url => url,
      depends => system.depends.as_deref().unwrap_or(""),
      sources => sources,
      sha1sums => sha1sums,
      build_section => build_section,
      package_section => package_section,
    };
    let pkgbuild = render_template(
      include_str!("../assets/templates/pkgbuild/pkgbuild.jinja"),
      &ctx,
    );
    let path = format!("{}/PKGBUILD", directory.display());
    std::fs::write(&path, pkgbuild).context(WriteResultSnafu { filename: path })
  }

  /// Builds the complete `Game` struct with all media paths and system-specific
  /// path applied. Used by both `build()` and `check_description_changed()`.
  fn make_game(&self, system: &System, lang: &[&str]) -> (Game, String) {
    let romname = self.normalize_name();
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

    apply_game_path(system, &mut game, &romname);
    (game, romname)
  }

  /// Returns `true` if generating description.xml now would produce content
  /// different from what is already on disk (or if the file doesn't exist yet).
  /// Does not write anything.
  pub fn check_description_changed(&self, system: &System, lang: &[&str]) -> bool {
    let (game, _) = self.make_game(system, lang);
    let directory = Path::new(&self.rom).with_extension("");
    let xml = generate_description_xml(&game);
    let existing = std::fs::read_to_string(directory.join("description.xml")).unwrap_or_default();
    existing != xml
  }

  /// Builds PKGBUILD + description.xml. Returns `true` if description.xml was
  /// written (new or updated content), `false` if it was already up-to-date.
  pub fn build(&mut self, system: &System, lang: &[&str], pkgver: u32) -> Result<bool> {
    let (mut game, romname) = self.make_game(system, lang);

    self.write_launcher(system, &mut game, &romname)?;

    let directory = Path::new(&self.rom).with_extension("");
    create_dir_all(&directory).ok();

    let description_changed = self.write_description_xml(&game, &directory)?;
    self.build_pkgbuild(system, &game, pkgver)?;
    Ok(description_changed)
  }
}
