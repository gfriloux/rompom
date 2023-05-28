use serde_derive::{Deserialize, Serialize};
use snafu::Snafu;
use chrono::prelude::*;

use screenscraper::jeuinfo::{JeuInfo,GenericIdText};

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
   pub region:      String,
   pub image:       Option<String>,
   pub thumbnail:   Option<String>,
   pub video:       Option<String>,
   pub marquee:     Option<String>,
   pub screenshot:  Option<String>,
   pub wheel:       Option<String>,
   pub manual:      Option<String>,
}

impl Game {
   pub fn french(jeu: &Option<JeuInfo>, path: &String) -> Result<Game> {
      let fulldate;
      let rating;
      let v = vec!["fr", "eu", "en", "us", "wor", "jp", "ss"];

      let name     = match jeu {
        Some(x) => x.find_name(&v),
        None    => "".to_string()
      };
      let desc     = match jeu {
        Some(x) => x.find_desc(&v),
        None    => "".to_string()
      };
      let ss_date  = match jeu {
        Some(x) => x.find_date(&v),
        None    => "".to_string()
      };
      let genre    = match jeu {
        Some(x) => x.find_genre(&v),
        None    => "".to_string()
      };
      let joueurs  = match jeu {
        Some(x) => {
          match &x.joueurs {
            Some(y) => y.text.clone(),
            None    =>  "Unknown".to_string()
          }
        },
        None   => "Unknown".to_string()
      };
      let mut region   = "".to_string();

      rating = match jeu {
        Some(x) => {
          match &x.note {
            Some(y) => y.text.parse::<f32>().unwrap_or(0.0) / 20.0,
            None    => 0.0
          }
        }
        None    => 0.8
      };

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
    region = match jeu {
      Some(x) => {
        match &x.rom {
          Some(y) => {
            match &y.romregions {
              Some(z) => z.to_string(),
              None    => "".to_string()
            }
          },
          None    => "".to_string()
        }
      },
      None    => "".to_string()
    };

      Ok(Game {
         path:        format!("./{}", path),
         name,
         desc,
         rating,
         releasedate: dt.format("%Y%m%dT%H%M%S").to_string(),
         developer:   match jeu {
          Some(x) => x.developpeur.as_ref().unwrap_or(&GenericIdText { id: "0".to_string(), text: "Unknown".to_string() }).text.clone(),
          None    => "Unknown".to_string()
         },
         publisher:   match jeu {
          Some(x) => x.editeur.as_ref().unwrap_or(&GenericIdText { id: "0".to_string(), text: "Unknown".to_string() }).text.clone(),
          None    => "Unknown".to_string()
         },
         genre,
         players:     joueurs,
         region,
         image:       None,
         thumbnail:   None,
         video:       None,
         marquee:     None,
         screenshot:  None,
         wheel:       None,
		 manual:      None,
      })
   }
}
