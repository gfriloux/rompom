use minijinja::{context, Environment};
use serde::Serialize;
use snafu::{ResultExt, Snafu};
use std::{fs::create_dir_all, path::Path};

use super::conf::System;
use super::emulationstation::Game;
use crate::hash::sha1_file;
use screenscraper::jeuinfo::{JeuInfo, Media};

/// The eight downloadable assets, in the order the grid shows them, each with the
/// ScreenScraper media names to try for it.
///
/// `ui::MEDIA_ICONS` fixes that order for the columns, and this list follows it — a
/// test holds the two together, because the dots are indexed by position and a list out
/// of step would attribute an asset to the wrong column.
///
/// `description` is not here: it comes from the synopsis, which is text on the game
/// rather than a file to fetch. `video` is the only asset with a fallback —
/// ScreenScraper serves a re-encoded copy when it has one, the raw upload otherwise.
///
/// This list used to be written out five times across four files, in two different
/// orders, and the `video` fallback twice. Adding an asset meant five coherent edits,
/// and one missed shifted the dots or dropped the file.
pub(crate) const MEDIA_KINDS: [(&str, &[&str]); 8] = [
  ("video", &["video-normalized", "video"]),
  ("image", &["sstitle"]),
  ("thumbnail", &["box-2D"]),
  ("screenshot", &["ss"]),
  ("bezel", &["bezel-16-9"]),
  ("marquee", &["marquee"]),
  ("wheel", &["wheel"]),
  ("manual", &["manuel"]),
];

#[derive(Default, Clone)]
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

impl Medias {
  /// Every asset paired with the name it is known by outside ScreenScraper, in the
  /// canonical order. The one way to walk these fields.
  pub(crate) fn iter(&self) -> impl Iterator<Item = (&'static str, Option<&Media>)> + '_ {
    [
      ("video", self.video.as_ref()),
      ("image", self.image.as_ref()),
      ("thumbnail", self.thumbnail.as_ref()),
      ("screenshot", self.screenshot.as_ref()),
      ("bezel", self.bezel.as_ref()),
      ("marquee", self.marquee.as_ref()),
      ("wheel", self.wheel.as_ref()),
      ("manual", self.manual.as_ref()),
    ]
    .into_iter()
  }

  /// Picks the assets out of a ScreenScraper result — first name that answers wins.
  fn from_jeu(jeu: &mut JeuInfo) -> Self {
    let pick = |kind: &str| -> Option<Media> {
      MEDIA_KINDS
        .iter()
        .find(|(k, _)| *k == kind)
        .and_then(|(_, names)| names.iter().find_map(|name| jeu.media(name)))
    };
    Medias {
      video: pick("video"),
      image: pick("image"),
      thumbnail: pick("thumbnail"),
      screenshot: pick("screenshot"),
      bezel: pick("bezel"),
      marquee: pick("marquee"),
      wheel: pick("wheel"),
      manual: pick("manual"),
    }
  }
}

pub struct Package {
  /// Logical/virtual ROM name (without disc indicator for multi-disc games).
  pub rom: String,
  /// Actual disc-1 filename (equals `rom` for single-disc games).
  pub disc1_filename: String,
  pub rom_url: String,
  pub hash: String,
  pub jeu: Option<JeuInfo>,
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

/// The public media URL — the only one that may leave this process.
///
/// ScreenScraper hands back a `mediaJeu.php` API call in `Media::url`, with `devid`,
/// `devpassword`, `ssid` and `sspassword` in the query string. That URL is unusable on
/// two counts: it cannot go into a PKGBUILD, which is published, and it must not be what
/// rompom fetches either — pulling every asset of every ROM through the API is how an
/// account gets rate-limited off ScreenScraper. The direct path under
/// `screenscraper.fr/medias/` bypasses the API; it wants a `Referer`, which the
/// `screenscraper` library sends on every media request.
///
/// Built once and used twice: for the PKGBUILD `sources`, and for rompom's own download.
///
/// The file name is `{type}({region}).{format}` — both parts come straight off the
/// `Media`, so there is nothing to parse out of the credentialed URL and nothing to
/// guess per asset kind. The region goes in **parentheses**; gluing it to the type, as
/// four of the eight assets used to, gives a 404:
///
/// ```text
/// …/medias/3/65388/sstitlejp.png    404
/// …/medias/3/65388/sstitle(jp).png  200
/// ```
///
/// An asset with no region — a video, a marquee — has no parentheses at all.
pub(crate) fn media_url(system_id: u32, jeu_id: &str, m: &Media) -> String {
  let slug = sanitize_token(&m.name);
  let ext = media_ext(&m.format);
  let file = match m.region.as_deref().map(sanitize_token) {
    Some(region) if !region.is_empty() => format!("{}({}).{}", slug, region, ext),
    _ => format!("{}.{}", slug, ext),
  };
  format!(
    "https://screenscraper.fr/medias/{}/{}/{}",
    system_id, jeu_id, file
  )
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

/// The name a media asset is filed under, both in the PKGBUILD `sources` and on disk.
///
/// The two must agree or `makepkg` looks for a file nobody wrote, which is why there is
/// one function rather than a rule remembered in two places.
///
/// The result is joined onto the ROM's output directory, and `Path::join` happily walks
/// out of it: a ScreenScraper format of `png/../../x` would have written outside the
/// tree. `media_ext` whitelists the extension, so what comes back is always a single
/// path component.
pub(crate) fn media_filename(kind: &str, format: &str) -> String {
  match kind {
    "video" => "video.mp4".to_string(),
    "manual" => "manual.pdf".to_string(),
    _ => format!("{}.{}", kind, media_ext(format)),
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
    let stem = Path::new(&self.rom)
      .file_stem()
      .and_then(|s| s.to_str())
      .unwrap_or(&self.rom);

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
      Some(ref mut x) => Medias::from_jeu(x),
      None => Medias::default(),
    };
    Ok(Package {
      rom: file.to_string(),
      disc1_filename: disc1_filename.to_string(),
      rom_url: url.to_string(),
      hash: hash.to_string(),
      jeu,
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

    // Media sources, in canonical order. Both arrays are appended in the same breath,
    // which is what keeps `sha1sums[n]` the checksum of `sources[n]` — the eight blocks
    // this replaced each had to remember to do it. `format` and `region` come straight
    // from ScreenScraper and end up in filenames, so `media_filename` whitelists them.
    for (kind, media) in self.medias.iter() {
      if let Some(m) = media {
        sources.push(shell_quote(&format!(
          "{}::{}",
          media_filename(kind, &m.format),
          media_url(system.id, &jeu_id, m)
        )));
        sha1sums.push(shell_quote(&sanitize_sha1(&m.sha1)));
      }
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

  // ── media_url ────────────────────────────────────────────────────────────

  /// A `Media` as ScreenScraper returns it. `url` is the `mediaJeu.php` call, with the
  /// credentials this function exists to keep out of everything downstream — and which
  /// it no longer even reads.
  fn api_media(name: &str, format: &str, region: Option<&str>) -> Media {
    Media {
      name: name.to_string(),
      parent: "jeu".to_string(),
      url: "https://api.screenscraper.fr/api2/mediaJeu.php?devid=x&devpassword=y&ssid=z\
            &sspassword=w&systemeid=3&jeuid=65388&media=whatever"
        .to_string(),
      region: region.map(str::to_string),
      crc: String::new(),
      md5: String::new(),
      sha1: String::new(),
      size: None,
      format: format.to_string(),
    }
  }

  /// Every one of these was checked against the server. The region goes in parentheses;
  /// gluing it to the type — which is what four of the eight assets used to do — is a
  /// 404, and the download then failed on every ROM that had one.
  ///
  /// ```text
  /// …/medias/3/65388/sstitlejp.png    404      …/3/65388/sstitle(jp).png   200
  /// …/medias/3/65388/wheeljp.png      404      …/3/65388/wheel(jp).png     200
  /// …/medias/3/134960/manueleu.pdf    404      …/3/134960/manuel(us).pdf   200
  /// ```
  #[test]
  fn the_media_url_puts_the_region_in_parentheses() {
    let base = "https://screenscraper.fr/medias/3/65388";
    for (name, format, region, expected) in [
      (
        "sstitle",
        "png",
        Some("jp"),
        format!("{}/sstitle(jp).png", base),
      ),
      (
        "box-2D",
        "png",
        Some("jp"),
        format!("{}/box-2D(jp).png", base),
      ),
      (
        "wheel",
        "png",
        Some("jp"),
        format!("{}/wheel(jp).png", base),
      ),
      (
        "manuel",
        "pdf",
        Some("jp"),
        format!("{}/manuel(jp).pdf", base),
      ),
      ("ss", "png", Some("jp"), format!("{}/ss(jp).png", base)),
      (
        "bezel-16-9",
        "png",
        Some("wor"),
        format!("{}/bezel-16-9(wor).png", base),
      ),
    ] {
      assert_eq!(
        media_url(3, "65388", &api_media(name, format, region)),
        expected,
        "{}",
        name
      );
    }
  }

  /// An asset with no region has no parentheses either — a bare `video.mp4`. Checked:
  /// both `video.mp4` and `video-normalized.mp4` answer 200.
  #[test]
  fn an_asset_without_a_region_has_no_parentheses() {
    let base = "https://screenscraper.fr/medias/3/65388";
    assert_eq!(
      media_url(3, "65388", &api_media("video", "mp4", None)),
      format!("{}/video.mp4", base)
    );
    assert_eq!(
      media_url(3, "65388", &api_media("video-normalized", "mp4", None)),
      format!("{}/video-normalized.mp4", base)
    );
    // An empty region is the same as none, not `sstitle().png`.
    assert_eq!(
      media_url(3, "65388", &api_media("marquee", "png", Some(""))),
      format!("{}/marquee.png", base)
    );
  }

  /// The URL goes into a published PKGBUILD, so nothing of `Media::url` may survive
  /// into it — and nothing does: it is not read at all any more.
  #[test]
  fn the_media_url_never_carries_the_credentials() {
    let url = media_url(3, "65388", &api_media("sstitle", "png", Some("jp")));
    for secret in ["devid", "devpassword", "ssid", "sspassword", "mediaJeu"] {
      assert!(!url.contains(secret), "leaks {}", secret);
    }
  }

  use super::*;

  // ── the canonical order ──────────────────────────────────────────────────

  /// `MEDIA_KINDS` and `Medias::iter()` are two lists of the same eight assets, and the
  /// second one is what every consumer walks. They have to agree, name for name and
  /// position for position, or an asset gets fetched under one name and filed under
  /// another.
  #[test]
  fn the_table_and_the_struct_walk_the_same_assets_in_the_same_order() {
    let table: Vec<&str> = MEDIA_KINDS.iter().map(|(kind, _)| *kind).collect();
    let walked: Vec<&str> = Medias::default().iter().map(|(kind, _)| kind).collect();

    assert_eq!(table, walked);
  }

  /// And both follow the columns on screen. The media dots are an array indexed by
  /// position, so a list out of step with `MEDIA_ICONS` would light the bezel column
  /// for a screenshot — which is exactly what the two competing orders used to risk.
  #[test]
  fn the_canonical_order_is_the_one_the_grid_shows() {
    let columns: Vec<&str> = crate::ui::media_icons()
      .iter()
      .map(|(kind, _)| *kind)
      .skip(1) // description is text on the game, not a file to fetch
      .collect();
    let table: Vec<&str> = MEDIA_KINDS.iter().map(|(kind, _)| *kind).collect();

    assert_eq!(table, columns);
  }

  /// ScreenScraper serves a re-encoded video when it has one and the raw upload
  /// otherwise, so `video` is the single asset with more than one name to try. The
  /// fallback used to be written out twice, here and in the modal projection.
  #[test]
  fn only_the_video_has_a_fallback_name() {
    for (kind, names) in MEDIA_KINDS {
      let expected = if kind == "video" { 2 } else { 1 };
      assert_eq!(names.len(), expected, "{}", kind);
    }
  }

  // ── media_filename ───────────────────────────────────────────────────────

  /// The destination is built as `directory.join(media_filename(...))`, and Path::join
  /// resolves `..` against the directory rather than rejecting it. Before the fix,
  /// a format of `png/../../x` produced `image.png/../../x`, which lands two levels
  /// above the ROM's output directory.
  #[test]
  fn media_filename_stays_inside_the_output_directory() {
    let directory = std::path::PathBuf::from("/out/roms/sonic");

    for hostile in [
      "png/../../x",
      "../../etc/passwd",
      "png/../..",
      "/etc/passwd",
      "png\\..\\..",
    ] {
      let name = media_filename("image", hostile);
      assert!(
        !name.contains('/') && !name.contains('\\') && !name.contains(".."),
        "format {hostile:?} produced {name:?}"
      );

      // One path component, and the join cannot leave the directory.
      let dest = directory.join(&name);
      assert_eq!(dest.parent(), Some(directory.as_path()));
      assert!(dest.starts_with(&directory));
    }
  }

  /// The fixed kinds keep their own extension, whatever ScreenScraper claims.
  #[test]
  fn media_filename_keeps_the_canonical_names() {
    assert_eq!(media_filename("video", "../../x"), "video.mp4");
    assert_eq!(media_filename("manual", "../../x"), "manual.pdf");
    assert_eq!(media_filename("image", "png"), "image.png");
    assert_eq!(media_filename("thumbnail", "jpg"), "thumbnail.jpg");
  }

  /// A format that whitelists down to nothing must still yield a usable name, and the
  /// same one the PKGBUILD source entry uses.
  #[test]
  fn media_filename_falls_back_when_the_format_is_unusable() {
    assert_eq!(media_filename("image", "../.."), "image.bin");
    assert_eq!(media_filename("image", ""), "image.bin");
  }

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

  /// A fully scraped game: every one of the eight assets present, each with a sha1 of
  /// its own so the snapshot pins `sha1sums` against the `sources` it lines up with.
  ///
  /// `rom` carries the scratch path because it is what the output directory is derived
  /// from, while `disc1_filename` stays a bare basename — in a real run both are the
  /// same relative name, and putting a temp path in the snapshot would make it depend on
  /// the machine.
  fn fully_scraped_package(directory: &Path) -> Package {
    let asset = |name: &str, format: &str, region: Option<&str>, digit: char| Media {
      sha1: std::iter::repeat_n(digit, 40).collect(),
      ..api_media(name, format, region)
    };
    Package {
      rom: directory
        .join("Sonic the Hedgehog.zip")
        .display()
        .to_string(),
      disc1_filename: "Sonic the Hedgehog.zip".to_string(),
      rom_url: "https://archive.invalid/megadrive/Sonic the Hedgehog.zip".to_string(),
      hash: SHA.to_string(),
      jeu: Some(
        serde_json::from_str(
          r#"{"id":"65388","noms":[{"region":"wor","text":"Sonic"}],
              "topstaff":"0","rotation":"0","medias":[]}"#,
        )
        .expect("fixture should deserialise as a JeuInfo"),
      ),
      medias: Medias {
        video: Some(asset("video-normalized", "mp4", None, '1')),
        image: Some(asset("sstitle", "png", Some("wor"), '2')),
        thumbnail: Some(asset("box-2D", "png", Some("wor"), '3')),
        screenshot: Some(asset("ss", "png", Some("wor"), '4')),
        bezel: Some(asset("bezel-16-9", "png", Some("wor"), '5')),
        marquee: Some(asset("marquee", "png", None, '6')),
        wheel: Some(asset("wheel", "png", Some("wor"), '7')),
        manual: Some(asset("manuel", "pdf", Some("eu"), '8')),
      },
      extra_discs: Vec::new(),
    }
  }

  /// Pins the exact bytes rompom writes to a PKGBUILD.
  ///
  /// `sources` and `sha1sums` are two parallel arrays that `makepkg` matches by
  /// position: entry *n* of one is the checksum of entry *n* of the other, and nothing
  /// in the file says so. Every asset is emitted by its own block today, so the pairing
  /// holds only for as long as each block remembers to push to both. This is what makes
  /// that pairing something a test can see.
  #[test]
  fn pkgbuild_snapshot() {
    let scratch = scratch_dir("pkgbuild-snapshot");
    let mut package = fully_scraped_package(scratch.path());
    let directory = scratch.path().join("Sonic the Hedgehog");
    std::fs::create_dir_all(&directory).unwrap();
    // build_pkgbuild reads this back to checksum it, so its content decides one line of
    // the snapshot. `sha1sum` of the single byte "x".
    std::fs::write(directory.join("description.xml"), "x").unwrap();

    package
      .build_pkgbuild(&system(1), &sample_game(), 3)
      .unwrap();

    let expected = [
      "pkgname=('test-rom-sonicthehedgehog')",
      "_romname='sonicthehedgehog'",
      "pkgver=3",
      "pkgrel=1",
      "pkgdesc='Sonic & Knuckles <Special>'",
      "arch=('any')",
      "url='https://screenscraper.fr/gameinfos.php?gameid=65388'",
      "license=('All rights reserved')",
      "source=(",
      "  'Sonic the Hedgehog.zip::https://archive.invalid/megadrive/Sonic the Hedgehog.zip'",
      "  'description.xml'",
      "  'video.mp4::https://screenscraper.fr/medias/1/65388/video-normalized.mp4'",
      "  'image.png::https://screenscraper.fr/medias/1/65388/sstitle(wor).png'",
      "  'thumbnail.png::https://screenscraper.fr/medias/1/65388/box-2D(wor).png'",
      "  'screenshot.png::https://screenscraper.fr/medias/1/65388/ss(wor).png'",
      "  'bezel.png::https://screenscraper.fr/medias/1/65388/bezel-16-9(wor).png'",
      "  'marquee.png::https://screenscraper.fr/medias/1/65388/marquee.png'",
      "  'wheel.png::https://screenscraper.fr/medias/1/65388/wheel(wor).png'",
      "  'manual.pdf::https://screenscraper.fr/medias/1/65388/manuel(eu).pdf'",
      ")",
      // The assets were given ascending sha1s in canonical order, so this array reading
      // 1 to 8 in order is the pairing with `sources` holding.
      "sha1sums=(",
      &format!("  '{}'", SHA),
      "  '11f6ad8ec52a2984abaafd7c3b516503785c2072'",
      "  '1111111111111111111111111111111111111111'",
      "  '2222222222222222222222222222222222222222'",
      "  '3333333333333333333333333333333333333333'",
      "  '4444444444444444444444444444444444444444'",
      "  '5555555555555555555555555555555555555555'",
      "  '6666666666666666666666666666666666666666'",
      "  '7777777777777777777777777777777777777777'",
      "  '8888888888888888888888888888888888888888'",
      ")",
      "",
      "build()",
      "{",
      "  true",
      "}",
      "",
      "",
    ]
    .join("\n");

    // Everything up to `package()`. The install section interpolates the ROM's own path,
    // which here is a scratch directory named after the process — a real run passes a
    // bare basename. What this snapshot is for stops at `sha1sums`.
    let written = std::fs::read_to_string(directory.join("PKGBUILD")).unwrap();
    let (head, _install) = written
      .split_once("package()")
      .expect("a PKGBUILD always has a package() section");
    assert_eq!(head, expected);
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
