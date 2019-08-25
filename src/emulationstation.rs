use snafu::Snafu;
use chrono::prelude::*;

use jeuinfos::{
   GenericIdText,
   JeuInfos
};

#[derive(Debug, Snafu)]
pub enum Error {

}

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Serialize, Deserialize, Debug)]
pub struct Game {
   pub path:        String,
   pub name:        String,
   pub desc:        String,
   pub rating:      f32,
   pub releasedate: String,
   pub developer:   String,
   pub publisher:   String,
   pub genre:       String,
   pub players:     String,
   pub image:       Option<String>,
   pub thumbnail:   Option<String>,
   pub video:       Option<String>
}

impl Game {
   pub fn french(jeu: &JeuInfos, path: &String) -> Result<Game> {
      let fulldate;
      let rating;
      let v = vec!["fr", "eu", "en", "us", "wor", "jp", "ss"];

      let name     = jeu.find_name(&v);
      let desc     = jeu.find_desc(&v);
      let ss_date  = jeu.find_date(&v);
      let genre    = jeu.find_genre(&v);
      let joueurs  = match &jeu.joueurs {
         Some(x) => { x.text.clone()        }
         None    => { "Unknown".to_string() }
      };

      if let Some(ref x) = &jeu.note {
         rating = x.text.parse::<f32>().unwrap_or(0.0) / 20.0;
      }
      else {
         rating = 0.0;
      }

      if ss_date.len() == 4 {
         fulldate = format!("{}-01-01 00:00:00 +00:00", ss_date);
      }
      else if ss_date.len() == 10 {
         if &ss_date == "0000-00-00" {
            fulldate = "1970-01-01 00:00:00 +00:00".to_string();
         }
         else {
            fulldate = format!("{} 00:00:00 +00:00", ss_date);
         }
      }
      else {
         fulldate = "1970-01-01 00:00:00 +00:00".to_string();
      }
      let dt       = DateTime::parse_from_str(&fulldate,
                                              "%Y-%m-%d %H:%M:%S %z").unwrap();

      Ok(Game {
         path:        format!("./{}", path),
         name:        name,
         desc:        desc,
         rating:      rating,
         releasedate: dt.format("%Y%m%dT%H%M%S").to_string(),
         developer:   jeu.developpeur.as_ref().unwrap_or(&GenericIdText { id: "0".to_string(), text: "Unknown".to_string() }).text.clone(),
         publisher:   jeu.editeur.as_ref().unwrap_or(&GenericIdText { id: "0".to_string(), text: "Unknown".to_string() }).text.clone(),
         genre:       genre,
         players:     joueurs,
         image:       None,
         thumbnail:   None,
         video:       None
      })
   }
}
