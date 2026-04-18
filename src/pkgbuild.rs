use std::{fmt, fs::File, io::Write, path::Path};

use snafu::{ResultExt, Snafu};

#[derive(Debug, Snafu)]
pub enum Error {
  #[snafu(display("Failed to write {}: {}", path, source))]
  Write {
    source: std::io::Error,
    path: String,
  },
}

type Result<T, E = Error> = std::result::Result<T, E>;

pub struct Pkgbuild {
  pub pkgname: String,
  pub romname: String,
  pub pkgver: String,
  pub pkgrel: u32,
  pub pkgdesc: String,
  pub url: String,
  pub depends: Option<String>,
  pub source: Vec<String>,
  pub sha1sums: Vec<String>,
  pub build: Vec<String>,
  pub package: Vec<String>,
}

impl Pkgbuild {
  pub fn write(&self, directory: &Path) -> Result<()> {
    let path = format!("{}/PKGBUILD", directory.display());
    println!("Writing {}", path);
    let mut f = File::create(&path).context(WriteSnafu { path: path.clone() })?;
    write!(f, "{}", self).context(WriteSnafu { path })?;
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
