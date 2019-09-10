use std::{
   io::copy,
   fs::File,
   path::{
      PathBuf,
      Path
   }
};

use snafu::{ResultExt, Snafu};

use screenscraper::Header;
use screenscraper::Response;
use conf::{
   Conf,
   System
};

#[derive(Serialize, Deserialize, Debug)]
pub struct GenericRegionText {
   pub region:          String,
   pub text:            String
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GenericIdText {
   pub id:              String,
   pub text:            String
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GenericText {
   pub text:            String
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GenericLangueText {
   pub langue:          String,
   pub text:            String
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Classification {
   #[serde(rename = "type")]
   pub name:            String,
   pub text:            String
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GenericObject {
   pub id:              String,
   pub principale:      String,
   pub parentid:        String,
   pub noms:            Vec<GenericLangueText>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Media {
   #[serde(rename = "type")]
   pub name:            String,
   pub parent:          String,
   pub url:             String,
   pub region:          Option<String>,
   pub crc:             String,
   pub md5:             String,
   pub sha1:            String,
   pub format:          String
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Rom {
   pub id:              Option<String>,
   pub romsize:         Option<String>,
   pub romfilename:     String,
   pub romregions:      Option<String>,
   pub romnumsupport:   Option<String>,
   pub romtotalsupport: Option<String>,
   pub romcloneof:      String,
   pub romcrc:          Option<String>,
   pub rommd5:          Option<String>,
   pub romsha1:         Option<String>,
   pub beta:            String,
   pub demo:            String,
   pub proto:           String,
   pub trad:            String,
   pub hack:            String,
   pub unl:             String,
   pub alt:             String,
   pub best:            String,
   pub netplay:         String
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JeuInfos {
   pub id:              String,
   pub romid:           Option<String>,
   pub notgame:         String,
   pub noms:            Vec<GenericRegionText>,
   pub systemeid:       String,
   pub systemenom:      String,
   pub editeur:         Option<GenericIdText>,
   pub developpeur:     Option<GenericIdText>,
   pub joueurs:         Option<GenericText>,
   pub note:            Option<GenericText>,
   pub topstaff:        String,
   pub rotation:        String,
   pub synopsis:        Option<Vec<GenericLangueText>>,
   pub classifications: Option<Vec<Classification>>,
   pub dates:           Option<Vec<GenericRegionText>>,
   pub genres:          Option<Vec<GenericObject>>,
   pub modes:           Option<Vec<GenericObject>>,
   pub familles:        Option<Vec<GenericObject>>,
   pub styles:          Option<Vec<GenericObject>>,
   pub medias:          Vec<Media>,
   pub roms:            Vec<Rom>,
   pub rom:             Option<Rom>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JeuInfosResult {
   pub header:   Header,
   pub response: Response
}

#[derive(Debug, Snafu)]
pub enum Error {
   #[snafu(display("Failed to download {}: {}", filename.display(), source))]
   DownloadFailed {
      filename: PathBuf,
      source: reqwest::Error,
   },

   #[snafu(display("Failed to write {}: {}", filename.display(), source))]
   WriteFailed {
      filename: PathBuf,
      source: std::io::Error,
   },

   #[snafu(display("Failed to read received data: {}", source))]
   ParseFailed {
      source: serde_json::Error,
   },
}

type Result<T, E = Error> = std::result::Result<T, E>;

impl JeuInfos {
   pub fn get(conf: &Conf, system: &System, game: &String, sha1: &String, rom: &String) -> Result<JeuInfos> {
      let     client = reqwest::Client::new();
      let     s;
      let     response: JeuInfosResult;
      let     url    = "https://www.screenscraper.fr/api2/jeuInfos.php";
      let mut query  = Vec::new();

      query.push(("devid"      , conf.screenscraper.dev.login.clone()));
      query.push(("devpassword", conf.screenscraper.dev.password.clone()));
      query.push(("softname"   , "RomPom".to_string()));
      query.push(("ssid"       , conf.screenscraper.user.login.clone()));
      query.push(("sspassword" , conf.screenscraper.user.password.clone()));
      query.push(("output"     , "json".to_string()));
      query.push(("systemeid"  , format!("{}", system.id)));
      query.push(("romnom"     , rom.to_string()));

      if system.checksum_disabled() == false {
         query.push(("sha1"       , sha1.clone()));
      }

      if game.as_str() != "0" {
         query.push(("gameid"     , game.to_string()));
      }

      let mut res    = client.get(url)
                             .query(&query)
                             .send().context(DownloadFailed { filename: PathBuf::from(&url) })?;

      s = res.text().context(DownloadFailed { filename: PathBuf::from(&url) })?;

      response = serde_json::from_str(&s).context(ParseFailed)?;

      Ok(response.response.jeu)
   }

   pub fn media(&mut self, name: &str) -> Option<Media> {
      let language = vec!["fr", "eu", "us", "wor", "jp", "ss"];

      for i in &language {

         for media in &self.medias {
            if let Some(x) = &media.region {
               if x != i {
                  continue;
               }
            }

            if &media.name == name {
               return Some(media.clone());
            }
         }
      }
      None
   }

   pub fn find_name(&self, fav: &Vec<&str>) -> String {
      if let Some(x) = &self.rom {
         if let Some(y) = &x.romregions {
            let lkt: Vec<&str> = y.split(',').collect();
            for i in lkt {
               match self.noms.iter().find(|x| &x.region == i) {
                  Some(x) => { return x.text.clone() },
                  None    => {                       }
               };
            }
         }
      }

      for i in fav {
         match self.noms.iter().find(|x| &x.region == i) {
            Some(x) => { return x.text.clone() },
            None    => {                       }
         };
      }
      "Unknown".to_string()
   }

   pub fn find_desc(&self, fav: &Vec<&str>) -> String {
      if let Some(ref x) = self.synopsis {
         for i in fav {
            for ref desc in x {
               if &desc.langue == i {
                  return desc.text.clone();
               }
            }
         }
      }
      "Unknown".to_string()
   }

   pub fn find_date(&self, fav: &Vec<&str>) -> String {
      if let Some(ref x) = self.dates {
         for i in fav {
            for ref date in x {
               if &date.region == i {
                  return date.text.clone();
               }
            }
         }
      }
      "Unknown".to_string()
   }

   pub fn find_genre(&self, fav: &Vec<&str>) -> String {
      if let Some(ref x) = &self.genres {
         for i in fav {
            for ref genre in x {
               if genre.principale != "1" {
                  continue;
               }

               for nom in &genre.noms {
                  if &nom.langue == i {
                     return nom.text.clone();
                  }
               }
            }
         }
      }
      "Unknown".to_string()
   }
}

impl Media {
   pub fn download(&mut self, path: &str) -> Result<()> {
      if Path::new(path).exists() == true {
         let hash = checksums::hash_file(Path::new(&path), checksums::Algorithm::SHA1);
         if &hash.to_lowercase() == &self.sha1.to_lowercase() {
            return Ok(());
         }
      }

      let mut src = reqwest::get(&self.url).context(DownloadFailed { filename: PathBuf::from(&self.url) })?;
      let mut dst = File::create(path).context(WriteFailed { filename: PathBuf::from(path) })?;
      copy(&mut src, &mut dst).context(WriteFailed { filename: PathBuf::from(path) })?;

      // We cannot trust SHA1 returned by SS, see issue #11
      let hash = checksums::hash_file(Path::new(&path), checksums::Algorithm::SHA1);

      if ! hash.is_empty() {
         self.sha1 = hash;
      }

      Ok(())
   }
}
