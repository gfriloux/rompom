use chrono::prelude::*;
use serde_derive::Serialize;

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

    let fulldate = if ss_date.len() == 4 {
      format!("{}-01-01 00:00:00 +00:00", ss_date)
    } else if ss_date.len() == 10 && ss_date != "0000-00-00" {
      format!("{} 00:00:00 +00:00", ss_date)
    } else {
      "1970-01-01 00:00:00 +00:00".to_string()
    };
    let dt = DateTime::parse_from_str(&fulldate, "%Y-%m-%d %H:%M:%S %z").unwrap();

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
      releasedate: dt.format("%Y%m%dT%H%M%S").to_string(),
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
