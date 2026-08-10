use chrono::{DateTime, Utc};
use serde::Serialize;

use screenscraper::jeuinfo::JeuInfo;

#[derive(Serialize, Debug)]
#[serde(rename = "game")]
pub struct Game {
  pub path: String,
  pub name: String,
  pub desc: String,
  pub rating: f32,
  pub releasedate: String,
  pub developer: String,
  pub publisher: String,
  pub genre: String,
  pub players: String,
  pub region: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub image: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub thumbnail: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub video: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub marquee: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub screenshot: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub wheel: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub manual: Option<String>,
}

/// Format EmulationStation expects in `<releasedate>`.
const ES_DATE_FORMAT: &str = "%Y%m%dT%H%M%S";

/// Converts a ScreenScraper release date into the EmulationStation format, falling back
/// to the epoch on anything it cannot parse.
///
/// ScreenScraper sends either a year (`"1994"`), a full date (`"1994-10-18"`), or a
/// placeholder (`"0000-00-00"`, empty). This used to test the string *length* and then
/// `unwrap()` the parse, which is not a validation at all: `"abcd"` is four characters
/// and `"2024-13-45"` is ten. Either one killed the worker thread mid-run.
fn release_date(ss_date: &str) -> String {
  let epoch = || {
    DateTime::<Utc>::UNIX_EPOCH
      .fixed_offset()
      .format(ES_DATE_FORMAT)
      .to_string()
  };

  let full = if ss_date.len() == 4 {
    format!("{}-01-01 00:00:00 +00:00", ss_date)
  } else if ss_date.len() == 10 && ss_date != "0000-00-00" {
    format!("{} 00:00:00 +00:00", ss_date)
  } else {
    return epoch();
  };

  match DateTime::parse_from_str(&full, "%Y-%m-%d %H:%M:%S %z") {
    Ok(dt) => dt.format(ES_DATE_FORMAT).to_string(),
    Err(_) => epoch(),
  }
}

impl Game {
  pub fn from_jeuinfo(jeu: &Option<JeuInfo>, path: &str, lang: &[&str]) -> Game {
    let region_fav = &["wor", "eu", "us", "fr", "jp", "ss"];

    let name = jeu
      .as_ref()
      .map(|x| x.find_name(region_fav))
      .unwrap_or_default();
    let desc = jeu.as_ref().map(|x| x.find_desc(lang)).unwrap_or_default();
    let ss_date = jeu
      .as_ref()
      .map(|x| x.find_date(region_fav))
      .unwrap_or_default();
    let genre = jeu.as_ref().map(|x| x.find_genre(lang)).unwrap_or_default();

    let players = jeu
      .as_ref()
      .and_then(|x| x.joueurs.as_ref())
      .map(|y| y.text.clone())
      .unwrap_or_else(|| "Unknown".to_string());

    let rating = match jeu {
      None => 0.8,
      Some(x) => x
        .note
        .as_ref()
        .and_then(|y| y.text.parse::<f32>().ok())
        .map(|n| n / 20.0)
        .unwrap_or(0.0),
    };

    let releasedate = release_date(&ss_date);

    let region = jeu
      .as_ref()
      .and_then(|x| x.rom.as_ref())
      .and_then(|y| y.regions.as_ref())
      .and_then(|z| z.regions_shortname.first())
      .cloned()
      .unwrap_or_default();

    let developer = jeu
      .as_ref()
      .and_then(|x| x.developpeur.as_ref())
      .map(|d| d.text.clone())
      .unwrap_or_else(|| "Unknown".to_string());

    let publisher = jeu
      .as_ref()
      .and_then(|x| x.editeur.as_ref())
      .map(|e| e.text.clone())
      .unwrap_or_else(|| "Unknown".to_string());

    Game {
      path: format!("./{}", path),
      name,
      desc,
      rating,
      releasedate,
      developer,
      publisher,
      genre,
      players,
      region,
      image: None,
      thumbnail: None,
      video: None,
      marquee: None,
      screenshot: None,
      wheel: None,
      manual: None,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const EPOCH: &str = "19700101T000000";

  /// The shapes ScreenScraper actually sends.
  #[test]
  fn release_date_handles_the_normal_shapes() {
    assert_eq!(release_date("1994"), "19940101T000000");
    assert_eq!(release_date("1994-10-18"), "19941018T000000");
  }

  /// Placeholders, which are common in the ScreenScraper data.
  #[test]
  fn release_date_falls_back_on_placeholders() {
    assert_eq!(release_date("0000-00-00"), EPOCH);
    assert_eq!(release_date(""), EPOCH);
  }

  /// The reason this stopped being an unwrap. Testing the string length is not a
  /// validation: each of these has the "right" length and used to panic on parse,
  /// killing the worker and hanging the run.
  #[test]
  fn release_date_survives_a_malformed_date_of_the_right_length() {
    assert_eq!(release_date("abcd"), EPOCH, "four characters, not a year");
    assert_eq!(release_date("2024-13-45"), EPOCH, "month 13, day 45");
    assert_eq!(release_date("1994-02-30"), EPOCH, "30 February");
    assert_eq!(release_date("xxxx-xx-xx"), EPOCH);
    assert_eq!(release_date("199X"), EPOCH);
  }

  /// Lengths the branches do not cover at all must still produce a usable date.
  #[test]
  fn release_date_falls_back_on_unexpected_lengths() {
    assert_eq!(release_date("1994-10"), EPOCH);
    assert_eq!(release_date("18 October 1994"), EPOCH);
  }
}
