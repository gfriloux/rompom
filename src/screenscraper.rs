use jeuinfos::JeuInfos;

#[derive(Serialize, Deserialize, Debug)]
pub struct Header {
   #[serde(rename = "APIversion")]
   pub apiversion:       String,

   #[serde(rename = "dateTime")]
   pub datetime:         String,

   #[serde(rename = "commandRequested")]
   pub commandrequested: String,
   pub success:          String,
   pub error:            String
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Ssuser {
   pub id:                 String,
   pub niveau:             String,
   pub contribution:       String,
   pub uploadsysteme:      String,
   pub uploadinfos:        String,
   pub romasso:            String,
   pub uploadmedia:        String,
   pub maxthreads:         String,
   pub maxdownloadspeed:   String,
   pub requeststoday:      String,
   pub visites:            String,
   pub datedernierevisite: String,
   pub favregion:          String
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Response {
   pub ssuser:   Ssuser,
   pub jeu:      JeuInfos,
}
