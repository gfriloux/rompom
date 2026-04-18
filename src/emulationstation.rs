use chrono::prelude::*;
use serde_derive::{Deserialize, Serialize};

use screenscraper::jeuinfo::JeuInfo;

#[derive(Serialize, Deserialize, Debug)]
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
  pub image: Option<String>,
  pub thumbnail: Option<String>,
  pub video: Option<String>,
  pub marquee: Option<String>,
  pub screenshot: Option<String>,
  pub wheel: Option<String>,
  pub manual: Option<String>,
}

impl Game {
  pub fn from_jeuinfo(jeu: &Option<JeuInfo>, path: &str) -> Game {
    let v = ["fr", "eu", "en", "us", "wor", "jp", "ss"];

    let name = jeu.as_ref().map(|x| x.find_name(&v)).unwrap_or_default();
    let desc = jeu.as_ref().map(|x| x.find_desc(&v)).unwrap_or_default();
    let ss_date = jeu.as_ref().map(|x| x.find_date(&v)).unwrap_or_default();
    let genre = jeu.as_ref().map(|x| x.find_genre(&v)).unwrap_or_default();

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
