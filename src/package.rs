use minijinja::{context, Environment};
use serde::Serialize;
use snafu::{ResultExt, Snafu};
use std::{fs::create_dir_all, path::Path};

use super::conf::System;
use super::emulationstation::Game;
use crate::hash::sha1_file;
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
  /// Logical/virtual ROM name (without disc indicator for multi-disc games).
  pub rom: String,
  /// Actual disc-1 filename (equals `rom` for single-disc games).
  pub disc1_filename: String,
  pub rom_url: String,
  pub hash: String,
  pub jeu: Option<JeuInfo>,
  pub name: String,
  pub medias: Medias,
  /// (filename, rom_url, sha1) for disc 2, 3, …  Empty for single-disc.
  pub extra_discs: Vec<(String, String, String)>,
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
///
/// `is_multi_disc` is true when the ROM is part of a multi-disc group; any
/// system (except OpenBOR) will then use a `.m3u` playlist as the game path.
fn apply_game_path(system: &System, game: &mut Game, romname: &str, is_multi_disc: bool) {
  match system.id {
    214 => game.path = format!("./{}.sh", game.name),
    22 | 57 => game.path = format!("./{}.m3u", romname),
    _ if is_multi_disc => game.path = format!("./{}.m3u", romname),
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

/// Wraps a value so a POSIX shell reads it back as exactly one literal token.
///
/// Inside single quotes every character is literal, so the only one needing care is
/// the single quote itself: close the quote, emit an escaped one, reopen. The returned
/// string **includes** its quotes — templates interpolate it bare, never inside quotes
/// of their own.
///
/// This is what stands between ScreenScraper data and `makepkg`. A game named
/// `$(id)` must reach the PKGBUILD as five characters, not as a command.
fn shell_quote(value: &str) -> String {
  format!("'{}'", value.replace('\'', r"'\''"))
}

/// Keeps only what is meaningful in a filename fragment coming from ScreenScraper —
/// a media format (`png`), a region (`wor`), a disc extension (`chd`).
///
/// These land in shell globs (`ls *.chd`) and in paths, where quoting alone would not
/// help: a `/` or a `..` would still traverse. A whitelist is the only reliable answer.
pub(crate) fn sanitize_token(value: &str) -> String {
  value
    .chars()
    .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
    .collect()
}

/// The extension to give a downloaded media asset, from the format ScreenScraper
/// reports.
///
/// Shared by the PKGBUILD `source` entries and the download destination on purpose:
/// if the two disagreed, makepkg would look for a file that is not there. The `bin`
/// fallback covers a format that whitelists down to nothing, so both sides still
/// agree on a name.
pub(crate) fn media_ext(format: &str) -> String {
  let clean = sanitize_token(format);
  if clean.is_empty() {
    "bin".to_string()
  } else {
    clean
  }
}

/// A `sha1sums` entry is 40 hex characters or it is corrupt.
///
/// Fails closed: anything else becomes an all-zero hash, so `makepkg` refuses the
/// package on an integrity mismatch. Substituting `SKIP` would be the opposite —
/// it disables the check on exactly the data we have reason to distrust.
fn sanitize_sha1(value: &str) -> String {
  let v = value.trim().to_ascii_lowercase();
  if v.len() == 40 && v.chars().all(|c| c.is_ascii_hexdigit()) {
    v
  } else {
    "0".repeat(40)
  }
}

/// Escapes a value used as the *pattern* of a `sed "s|pattern|replacement|"` running
/// inside double quotes — the Sega CD template rewrites the ROM name in description.xml
/// that way.
///
/// Two layers apply at once: the shell reads the double-quoted string first (`$`,
/// backtick, `\`, `"`), then sed reads the result as a basic regular expression
/// (`. * [ ] ^` and the `|` delimiter).
fn sed_pattern(value: &str) -> String {
  let mut out = String::with_capacity(value.len());
  for c in value.chars() {
    if matches!(
      c,
      '\\' | '"' | '$' | '`' | '|' | '.' | '*' | '[' | ']' | '^' | '/' | '&'
    ) {
      out.push('\\');
    }
    out.push(c);
  }
  out
}

/// Maps a Latin letter carrying a diacritic onto its ASCII base.
///
/// Without this, whitelisting would turn "Astérix" into "astrix". Game titles are
/// full of accents, and `pkgname` has to stay both readable and Arch-legal.
fn fold_latin(c: char) -> Option<&'static str> {
  Some(match c {
    'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => "a",
    'ç' => "c",
    'è' | 'é' | 'ê' | 'ë' => "e",
    'ì' | 'í' | 'î' | 'ï' => "i",
    'ñ' => "n",
    'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => "o",
    'ù' | 'ú' | 'û' | 'ü' => "u",
    'ý' | 'ÿ' => "y",
    'æ' => "ae",
    'œ' => "oe",
    'ß' => "ss",
    _ => return None,
  })
}

impl Package {
  /// Derives the package name from the ROM filename, keeping only characters that are
  /// legal in an Arch `pkgname`.
  ///
  /// This is a **whitelist**. The previous blocklist enumerated characters to strip and
  /// missed `"`, backtick, `\` and newline — all of which reach a double-quoted shell
  /// assignment in the PKGBUILD. A blocklist cannot be complete; this one cannot be
  /// escaped from.
  ///
  /// Titles written entirely in a non-Latin script (Japanese, for one) would normalize
  /// to nothing and collide with every other such title, so they fall back to the ROM
  /// hash, which is stable across runs.
  pub fn normalize_name(&self) -> String {
    let stem = Path::new(&self.name)
      .file_stem()
      .and_then(|s| s.to_str())
      .unwrap_or(&self.name);

    let mut out = String::with_capacity(stem.len());
    for c in stem.chars() {
      let lower = c.to_ascii_lowercase();
      match lower {
        'a'..='z' | '0'..='9' | '.' | '_' | '-' => out.push(lower),
        '&' => out.push_str("and"),
        '~' | '=' => out.push('-'),
        _ => {
          if let Some(folded) = fold_latin(c.to_lowercase().next().unwrap_or(c)) {
            out.push_str(folded);
          }
        }
      }
    }

    if out.is_empty() {
      let hash = sanitize_sha1(&self.hash);
      format!("rom-{}", &hash[..12])
    } else {
      out
    }
  }

  pub fn new(
    mut jeu: Option<JeuInfo>,
    file: &str,
    disc1_filename: &str,
    url: &str,
    hash: &str,
    extra_discs: Vec<(String, String, String)>,
  ) -> Result<Package> {
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
      disc1_filename: disc1_filename.to_string(),
      rom_url: url.to_string(),
      hash: hash.to_string(),
      jeu,
      name: file.to_string(),
      medias,
      extra_discs,
    })
  }

  /// Returns `true` when this ROM is part of a multi-disc group.
  pub fn is_multi_disc(&self) -> bool {
    !self.extra_discs.is_empty()
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

  /// Writes the system-specific launcher script into the ROM's own directory.
  ///
  /// It used to go to `./launcher`, in the process-wide current directory. Every ROM of
  /// an OpenBOR run therefore wrote to the same path, and the workers run in parallel:
  /// the file that survived belonged to whichever ROM finished last.
  fn write_launcher(
    &self,
    system: &System,
    game: &mut Game,
    romname: &str,
    directory: &Path,
  ) -> Result<()> {
    if system.id == 214 {
      let ctx = context! {
        rom => self.rom.replace("'", "'\\''"),
      };
      let launcher = render_template(
        include_str!("../assets/templates/launcher/openbor.jinja"),
        &ctx,
      );
      let path = directory.join("launcher");
      std::fs::write(&path, launcher).context(WriteResultSnafu {
        filename: path.display().to_string(),
      })?;
    }
    apply_game_path(system, game, romname, self.is_multi_disc());
    Ok(())
  }

  pub fn build_pkgbuild(&mut self, system: &System, game: &Game, pkgver: u32) -> Result<()> {
    let romname = self.normalize_name();
    let directory = Path::new(&self.rom).with_extension("");
    let jeu_id = sanitize_token(self.jeu.as_ref().map(|j| j.id.as_str()).unwrap_or(""));

    // Sources & checksums. Every entry is shell-quoted here rather than in the
    // template, so a template can never forget to quote one.
    let mut sources: Vec<String> = Vec::new();
    let mut sha1sums: Vec<String> = Vec::new();

    // Disc 1 (or the only disc for single-disc games).
    sources.push(shell_quote(&format!(
      "{}::{}",
      self.disc1_filename, self.rom_url
    )));
    sha1sums.push(shell_quote(&sanitize_sha1(&self.hash)));

    // Extra discs (disc 2, 3, …).
    for (disc_filename, disc_url, disc_sha1) in &self.extra_discs {
      sources.push(shell_quote(&format!("{}::{}", disc_filename, disc_url)));
      sha1sums.push(shell_quote(&sanitize_sha1(disc_sha1)));
    }

    sources.push(shell_quote("description.xml"));
    // Written moments ago by write_description_xml, so a read error here means something
    // is badly wrong with the output directory. sanitize_sha1 turns the empty string into
    // forty zeros, which makepkg rejects loudly — the one outcome worse than that would be
    // a PKGBUILD claiming a sum nobody computed.
    let description_sha1 =
      sha1_file(&directory.join("description.xml")).unwrap_or_else(|_| String::new());
    sha1sums.push(shell_quote(&sanitize_sha1(&description_sha1)));

    // Media sources. `format` and `region` come straight from ScreenScraper and end up
    // in filenames, so they go through the token whitelist before anything else.
    if let Some(ref x) = self.medias.video {
      sources.push(shell_quote(&format!(
        "video.mp4::https://screenscraper.fr/medias/{}/{}/video.mp4",
        system.id, jeu_id
      )));
      sha1sums.push(shell_quote(&sanitize_sha1(&x.sha1)));
    }
    if let Some(ref x) = self.medias.bezel {
      let fmt = media_ext(&x.format);
      let region = sanitize_token(x.region.as_deref().unwrap_or("wor"));
      sources.push(shell_quote(&format!(
        "bezel.{}::https://screenscraper.fr/medias/{}/{}/bezel-16-9({}).{}",
        fmt, system.id, jeu_id, region, fmt
      )));
      sha1sums.push(shell_quote(&sanitize_sha1(&x.sha1)));
    }
    if let Some(ref x) = self.medias.image {
      let fmt = media_ext(&x.format);
      let region = sanitize_token(media_region(&x.url));
      sources.push(shell_quote(&format!(
        "image.{}::https://screenscraper.fr/medias/{}/{}/{}.{}",
        fmt, system.id, jeu_id, region, fmt
      )));
      sha1sums.push(shell_quote(&sanitize_sha1(&x.sha1)));
    }
    if let Some(ref x) = self.medias.thumbnail {
      let fmt = media_ext(&x.format);
      let region = sanitize_token(media_region(&x.url));
      sources.push(shell_quote(&format!(
        "thumbnail.{}::https://screenscraper.fr/medias/{}/{}/{}.{}",
        fmt, system.id, jeu_id, region, fmt
      )));
      sha1sums.push(shell_quote(&sanitize_sha1(&x.sha1)));
    }
    if let Some(ref x) = self.medias.marquee {
      let fmt = media_ext(&x.format);
      sources.push(shell_quote(&format!(
        "marquee.{}::https://screenscraper.fr/medias/{}/{}/marquee.{}",
        fmt, system.id, jeu_id, fmt
      )));
      sha1sums.push(shell_quote(&sanitize_sha1(&x.sha1)));
    }
    if let Some(ref x) = self.medias.screenshot {
      let fmt = media_ext(&x.format);
      let region = sanitize_token(x.region.as_deref().unwrap_or("wor"));
      sources.push(shell_quote(&format!(
        "screenshot.{}::https://screenscraper.fr/medias/{}/{}/ss({}).{}",
        fmt, system.id, jeu_id, region, fmt
      )));
      sha1sums.push(shell_quote(&sanitize_sha1(&x.sha1)));
    }
    if let Some(ref x) = self.medias.wheel {
      let fmt = media_ext(&x.format);
      let region = sanitize_token(media_region(&x.url));
      sources.push(shell_quote(&format!(
        "wheel.{}::https://screenscraper.fr/medias/{}/{}/{}.{}",
        fmt, system.id, jeu_id, region, fmt
      )));
      sha1sums.push(shell_quote(&sanitize_sha1(&x.sha1)));
    }
    if let Some(ref x) = self.medias.manual {
      let region = sanitize_token(media_region(&x.url));
      sources.push(shell_quote(&format!(
        "manual.pdf::https://screenscraper.fr/medias/{}/{}/{}.pdf",
        system.id, jeu_id, region
      )));
      sha1sums.push(shell_quote(&sanitize_sha1(&x.sha1)));
    }

    // Extension of the disc files (used by multi-disc templates in a `ls *.ext` glob,
    // hence the whitelist rather than quoting).
    let disc_ext = sanitize_token(
      Path::new(&self.disc1_filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("zip"),
    );

    // System-specific build/package sections
    // `rom` is shell-quoted for the install commands; `rom_sed` is escaped for the
    // sed s|| expression the Sega CD template uses, a different context entirely.
    let sys_ctx = context! {
      dir => system.dir,
      rom => shell_quote(&self.rom),
      rom_sed => sed_pattern(&self.rom),
      ext => disc_ext,
    };
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
      _ if self.is_multi_disc() => (
        include_str!("../assets/templates/pkgbuild/multidisc-build.jinja"),
        include_str!("../assets/templates/pkgbuild/multidisc-package.jinja"),
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
      pkgname => shell_quote(&format!("{}{}", system.basename, romname)),
      romname => shell_quote(&romname),
      pkgver => pkgver,
      pkgrel => 1_u32,
      pkgdesc => shell_quote(&game.name),
      url => shell_quote(&url),
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

    apply_game_path(system, &mut game, &romname, self.is_multi_disc());
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

    // The directory has to exist before the launcher is written into it.
    let directory = Path::new(&self.rom).with_extension("");
    create_dir_all(&directory).ok();

    self.write_launcher(system, &mut game, &romname, &directory)?;

    let description_changed = self.write_description_xml(&game, &directory)?;
    self.build_pkgbuild(system, &game, pkgver)?;
    Ok(description_changed)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// A game exercising every branch of the serialization: populated and skipped
  /// `Option` fields, and characters XML must escape (`&`, `<`, `>`, `"`).
  fn sample_game() -> Game {
    Game {
      path: "./Sonic & Knuckles.zip".to_string(),
      name: "Sonic & Knuckles <Special>".to_string(),
      desc: "A \"blue\" hedgehog & his friend > all.".to_string(),
      rating: 0.85,
      releasedate: "19941018T000000".to_string(),
      developer: "Sega".to_string(),
      publisher: "Sega".to_string(),
      genre: "Plate-forme".to_string(),
      players: "1-2".to_string(),
      region: "wor".to_string(),
      image: Some("./image.png".to_string()),
      thumbnail: None,
      video: Some("./video.mp4".to_string()),
      marquee: None,
      screenshot: None,
      wheel: None,
      manual: None,
    }
  }

  /// Pins the exact bytes rompom writes to `description.xml`.
  ///
  /// EmulationStation reads these files and `check_description_changed()` compares
  /// them byte for byte to decide whether to bump `pkgver`, so a silent change in
  /// quick-xml's output would rewrite and re-version every package on the next run.
  /// This snapshot was verified identical across the quick-xml 0.39.2 -> 0.41.0
  /// upgrade; if it moves, the change must be deliberate.
  #[test]
  fn description_xml_snapshot() {
    let expected = [
      "<game>",
      "  <path>./Sonic &amp; Knuckles.zip</path>",
      "  <name>Sonic &amp; Knuckles &lt;Special&gt;</name>",
      "  <desc>A \"blue\" hedgehog &amp; his friend &gt; all.</desc>",
      "  <rating>0.85</rating>",
      "  <releasedate>19941018T000000</releasedate>",
      "  <developer>Sega</developer>",
      "  <publisher>Sega</publisher>",
      "  <genre>Plate-forme</genre>",
      "  <players>1-2</players>",
      "  <region>wor</region>",
      "  <image>./image.png</image>",
      "  <video>./video.mp4</video>",
      "</game>",
    ]
    .join("\n");

    assert_eq!(generate_description_xml(&sample_game()), expected);
  }

  fn pkg(rom_name: &str, hash: &str) -> Package {
    Package {
      rom: rom_name.to_string(),
      disc1_filename: rom_name.to_string(),
      rom_url: "https://example.invalid/rom.zip".to_string(),
      hash: hash.to_string(),
      jeu: None,
      name: rom_name.to_string(),
      medias: Medias::default(),
      extra_discs: Vec::new(),
    }
  }

  const SHA: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";

  /// The blocklist this replaced stripped `$` but left backtick, `"`, `\` and newline,
  /// all of which reached a double-quoted assignment in the PKGBUILD. Before the fix
  /// this produced `sonic`id`` and `game";id;"`.
  #[test]
  fn normalize_name_drops_every_shell_metacharacter() {
    for hostile in [
      "sonic`id`.zip",
      "game\";id;\".zip",
      "game$(id).zip",
      "game\\x.zip",
      "game\nid.zip",
      "game|id.zip",
      "game>out.zip",
    ] {
      let got = pkg(hostile, SHA).normalize_name();
      for bad in ['`', '"', '$', '\\', '\n', '|', '>', '(', ')', ';', '\''] {
        assert!(
          !got.contains(bad),
          "{hostile:?} normalized to {got:?}, which still contains {bad:?}"
        );
      }
    }
  }

  /// Whitelisting must not mangle accented titles into unreadable stumps:
  /// "Astérix" has to stay "asterix", not become "astrix".
  #[test]
  fn normalize_name_folds_accents() {
    assert_eq!(
      pkg("Astérix & Obélix.zip", SHA).normalize_name(),
      "asterixandobelix"
    );
    assert_eq!(
      pkg("Pokémon Rouge.zip", SHA).normalize_name(),
      "pokemonrouge"
    );
  }

  /// A title written entirely in a non-Latin script whitelists down to nothing. Without
  /// a fallback every such ROM would share one package name and overwrite the others.
  #[test]
  fn normalize_name_falls_back_to_the_hash_when_nothing_survives() {
    let got = pkg("ソニック.zip", SHA).normalize_name();
    assert_eq!(got, format!("rom-{}", &SHA[..12]));
    assert_ne!(
      got,
      pkg("メトロイド.zip", "0".repeat(40).as_str()).normalize_name()
    );
  }

  /// The core guarantee: whatever ScreenScraper sends, the shell sees one literal token.
  #[test]
  fn shell_quote_neutralizes_command_substitution() {
    assert_eq!(shell_quote("$(id)"), "'$(id)'");
    assert_eq!(shell_quote("`id`"), "'`id`'");
    assert_eq!(shell_quote("it's"), r"'it'\''s'");
    // The escape must not be defeatable by closing the quote first.
    assert_eq!(shell_quote("';id;'"), r"''\'';id;'\'''");
  }

  /// `format` and `region` land in filenames and globs, where quoting does not stop a
  /// traversal — only a whitelist does.
  #[test]
  fn sanitize_token_strips_path_and_shell_characters() {
    assert_eq!(sanitize_token("png/../../etc"), "pngetc");
    assert_eq!(sanitize_token("png"), "png");
    assert_eq!(sanitize_token("../.."), "");
    assert_eq!(sanitize_token("wor;id"), "worid");
  }

  /// A malformed checksum must fail the build, never disable the check.
  #[test]
  fn sanitize_sha1_fails_closed() {
    assert_eq!(sanitize_sha1(SHA), SHA);
    assert_eq!(sanitize_sha1(&SHA.to_uppercase()), SHA);
    assert_eq!(sanitize_sha1("'; rm -rf /; '"), "0".repeat(40));
    assert_eq!(sanitize_sha1("deadbeef"), "0".repeat(40));
    assert_ne!(sanitize_sha1("nonsense"), "SKIP");
  }

  /// Sega CD rewrites the ROM name through sed; a `|` in a filename used to end the
  /// expression early, and a backtick reached the shell through the double quotes.
  #[test]
  fn sed_pattern_escapes_both_shell_and_regex() {
    assert_eq!(sed_pattern("a|b"), r"a\|b");
    assert_eq!(sed_pattern("a.b*"), r"a\.b\*");
    assert_eq!(sed_pattern("`id`"), r"\`id\`");
    assert_eq!(sed_pattern("$HOME"), r"\$HOME");
  }

  /// The template must interpolate pre-quoted values bare. Wrapping them in quotes of
  /// its own would nest one layer inside another and reopen the hole the escaping just
  /// closed — this is the check that the .jinja files and shell_quote agree.
  #[test]
  fn pkgbuild_template_does_not_requote_escaped_values() {
    let rendered = render_template(
      include_str!("../assets/templates/pkgbuild/pkgbuild.jinja"),
      &context! {
        pkgname => shell_quote("megadrive-sonic"),
        romname => shell_quote("sonic"),
        pkgver => 1_u32,
        pkgrel => 1_u32,
        pkgdesc => shell_quote("Sonic & Knuckles $(id) `id`"),
        url => shell_quote("https://example.invalid/"),
        depends => "",
        sources => vec![shell_quote("rom.zip::https://example.invalid/a'b")],
        sha1sums => vec![shell_quote(SHA)],
        build_section => "  true",
        package_section => "  true",
      },
    );

    assert!(rendered.contains("pkgdesc='Sonic & Knuckles $(id) `id`'"));
    assert!(rendered.contains("_romname='sonic'"));
    assert!(rendered.contains("pkgname=('megadrive-sonic')"));
    // The apostrophe in the URL must be broken out and re-quoted, not left bare.
    assert!(rendered.contains(r"'rom.zip::https://example.invalid/a'\''b'"));
    // No value may end up double-quoted: that is the layering mistake to catch.
    assert!(!rendered.contains("pkgdesc=\""));
    assert!(!rendered.contains("_romname=\""));
    assert!(!rendered.contains("''''"));
  }

  /// `skip_serializing_if` must drop absent media rather than emit empty tags:
  /// EmulationStation treats `<thumbnail></thumbnail>` as a path to a missing file.
  #[test]
  fn description_xml_omits_absent_medias() {
    let xml = generate_description_xml(&sample_game());
    for absent in ["thumbnail", "marquee", "screenshot", "wheel", "manual"] {
      assert!(!xml.contains(absent), "{absent} should not be serialized");
    }
  }

  // ── apply_game_path ──────────────────────────────────────────────────────

  fn system(id: u32) -> System {
    System {
      name: "test".to_string(),
      id,
      basename: "test-rom-".to_string(),
      depends: None,
      dir: "test".to_string(),
      source: None,
    }
  }

  /// The default: EmulationStation keeps whatever path the Game already carries. Most
  /// systems launch the ROM file directly, so touching this would break them all.
  #[test]
  fn an_ordinary_single_disc_game_keeps_its_path() {
    let mut game = sample_game();
    let before = game.path.clone();

    apply_game_path(&system(4), &mut game, "sonic", false);

    assert_eq!(game.path, before);
  }

  /// OpenBOR games are launched through a generated shell script, named after the game
  /// rather than after the normalised romname.
  #[test]
  fn openbor_points_at_a_shell_script_named_after_the_game() {
    let mut game = sample_game();
    let expected = format!("./{}.sh", game.name);

    apply_game_path(&system(214), &mut game, "sonic", false);

    assert_eq!(game.path, expected);
  }

  /// Saturn/PSX (22) and PS2 (57) always install their discs beside an .m3u playlist,
  /// single-disc releases included — the PKGBUILD templates build one either way.
  #[test]
  fn the_disc_based_systems_always_point_at_the_playlist() {
    for id in [22, 57] {
      let mut game = sample_game();
      apply_game_path(&system(id), &mut game, "final-fantasy-vii", false);
      assert_eq!(game.path, "./final-fantasy-vii.m3u", "system {}", id);
    }
  }

  /// And any other system does the same as soon as the game has more than one disc.
  #[test]
  fn a_multi_disc_game_points_at_the_playlist_on_any_system() {
    let mut game = sample_game();

    apply_game_path(&system(20), &mut game, "lunar", true);

    assert_eq!(game.path, "./lunar.m3u");
  }

  /// OpenBOR wins over the multi-disc rule: the script is what launches the game, and
  /// a .m3u would point EmulationStation at a playlist nothing produces.
  #[test]
  fn openbor_wins_over_the_multi_disc_rule() {
    let mut game = sample_game();
    let expected = format!("./{}.sh", game.name);

    apply_game_path(&system(214), &mut game, "beats-of-rage", true);

    assert_eq!(game.path, expected);
  }

  // ── read_pkgver ──────────────────────────────────────────────────────────

  /// The caller adds 1 to whatever comes back, so a wrong 0 here republishes an already
  /// published package as version 1 — pacman then refuses the downgrade and the fix
  /// never reaches the machine.
  #[test]
  fn read_pkgver_reads_the_version_a_previous_run_wrote() {
    let dir = scratch_dir("pkgver-present");
    std::fs::write(
      dir.path().join("PKGBUILD"),
      "pkgname=('x')\npkgver=7\npkgrel=1\n",
    )
    .unwrap();

    assert_eq!(read_pkgver(dir.path()), 7);
  }

  /// A directory with no package yet — the genuine first run.
  #[test]
  fn read_pkgver_is_zero_when_there_is_no_pkgbuild() {
    let dir = scratch_dir("pkgver-absent");

    assert_eq!(read_pkgver(dir.path()), 0);
  }

  /// A PKGBUILD that is there but says nothing usable. Falling back to 0 is the
  /// deliberate choice: the alternative is guessing, and the next build reports the
  /// mismatch loudly.
  #[test]
  fn read_pkgver_is_zero_when_the_line_is_missing_or_unusable() {
    for contents in [
      "pkgname=('x')\npkgrel=1\n",
      "pkgname=('x')\npkgver=\npkgrel=1\n",
      "pkgname=('x')\npkgver=1.2.3\n",
      "pkgname=('x')\npkgver=-4\n",
      "  pkgver=9\n",
    ] {
      let dir = scratch_dir("pkgver-unusable");
      std::fs::write(dir.path().join("PKGBUILD"), contents).unwrap();

      assert_eq!(read_pkgver(dir.path()), 0, "contents: {:?}", contents);
    }
  }

  /// Scratch directory removed on drop, so the tests leave nothing behind and can run
  /// concurrently with each other.
  fn scratch_dir(tag: &str) -> ScratchDir {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
      "rompom-pkg-test-{}-{}-{}",
      std::process::id(),
      tag,
      n
    ));
    std::fs::create_dir_all(&dir).unwrap();
    ScratchDir(dir)
  }

  struct ScratchDir(std::path::PathBuf);

  impl ScratchDir {
    fn path(&self) -> &Path {
      &self.0
    }
  }

  impl Drop for ScratchDir {
    fn drop(&mut self) {
      let _ = std::fs::remove_dir_all(&self.0);
    }
  }
}
