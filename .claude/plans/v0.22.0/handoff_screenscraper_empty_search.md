# Handoff — `screenscraper` : une recherche sans résultat fait échouer tout l'appel

**Dépôt** : `github.com/gfriloux/screenscraper` (local : `../screenscraper`)
**Demandé par** : rompom, constaté le 2026-08-13 pendant les tests manuels de v0.22.0
**Taille** : petit (~10 lignes + un test)
**Gravité** : **haute** — l'identification manuelle est inatteignable pour les ROMs qui
en ont le plus besoin.

---

## Le symptôme, côté rompom

Sur un run `nes` de 1992 ROMs, trois échecs, tous avec la même cause :

```
534  Dragon Ball Party 4 In 1.zip ✗   ScreenScraper sent a response rompom could not parse
```

C'est `ApiFailure::Malformed` — « une réponse 2xx que la lib n'a pas su parser »
(`lib.rs:77`).

## Ce que ScreenScraper renvoie réellement

`jeuRecherche.php`, systemeid 3, quand la recherche ne trouve rien : **HTTP 200**,
`header.success == "true"`, et un corps parfaitement valide en JSON —

```json
{ "header": { "success": "true", "error": "" },
  "response": { "serveurs": {...}, "ssuser": {...}, "jeux": [ {} ] } }
```

`jeux` est **une liste contenant un objet vide**. Pas une liste vide, pas un 404.
Reproductible et systématique :

| recherche (systemeid 3) | HTTP | `success` | `jeux` |
|---|---|---|---|
| `Dragon Ball Party 4 In 1` | 200 | true | `[{}]` |
| `Zzqqxx Nonexistent Game 9999` | 200 | true | `[{}]` |
| `Metroid` | 200 | true | 3 jeux complets |

Pour mémoire, `jeuInfos.php` sur la même ROM introuvable répond bien **404** avec un corps
`Erreur : Rom/Iso/Dossier non trouvée !` en texte brut — ce chemin-là est correctement
traité, il devient `ApiFailure::NotFound`. Seul `jeuRecherche` a cette forme.

## Pourquoi la lib échoue

`JeuInfo` (`src/jeuinfo.rs:91`) déclare cinq champs non optionnels : `id`, `noms`,
`topstaff`, `rotation`, `medias`. Vérifié en désérialisant le corps réel avec la struct
réelle (v0.8.0, crate jetable, pas une lecture du type) :

```
jeux is an array of 1
  [0] PARSE ERROR: missing field `id`
```

`parse_jeu_recherche()` désérialise `jeux` d'un bloc, donc **une** entrée vide fait échouer
**tout** l'appel : `api::Error::Parse` → `ApiFailure::Malformed`.

## Pourquoi c'est grave côté consommateur

`Malformed` n'est pas `is_retryable()`, donc rompom en fait un `StepError::Fatal`
(`worker/helpers.rs:37`, `handlers/discovery.rs:250`). Le step `LookupSS` échoue,
`skip_successors` marque `WaitModal` en `Skipped`, et **la modale d'identification ne
s'ouvre jamais**.

Les trois cas, dont un seul est cassé :

| sha1 connu de SS | recherche par nom | résultat |
|---|---|---|
| oui | — | identifié automatiquement. OK |
| non (404) | des candidats | modale avec candidats. OK |
| non (404) | **rien** | `[{}]` → parse → `Fatal` → **pas de modale du tout** |

C'est-à-dire : la saisie manuelle d'un game id est morte exactement pour les ROMs dont le
nom ne correspond à rien chez ScreenScraper — multicarts pirates, hacks, romsets
exotiques. Celles qui n'ont que ce recours.

**À noter** : ce cas a été *créé* par le correctif P1.1 de rompom (v0.17.0). Avant,
`jeu_recherche(...).unwrap_or_default()` avalait l'erreur et ouvrait une modale vide. P1.1
a eu raison d'arrêter d'avaler les pannes réseau — mais `[{}]` n'est pas une panne, c'est
« zéro résultat », et il est tombé dans le même sac. Le correctif appartient à la lib, pas
au consommateur : traiter `Malformed` comme « zéro candidat » côté rompom rouvrirait
précisément le trou que P1.1 a bouché.

## Correctif proposé

Traduire la convention de ScreenScraper là où elle est connue : dans
`parse_jeu_recherche()`. Une entrée qui n'est **pas** un objet JSON non vide n'est pas un
jeu ; c'est la façon dont l'API dit « rien trouvé ».

```rust
#[derive(Deserialize)]
struct ResponseJeuRecherche {
  /// ScreenScraper exprime « aucun résultat » par une liste d'**un objet vide**
  /// (HTTP 200, `success: "true"`), et non par une liste vide ni par un 404.
  /// Désérialiser en `Vec<JeuInfo>` fait donc échouer tout l'appel sur
  /// `missing field \`id\``, et le consommateur reçoit `Malformed` — indiscernable
  /// d'une vraie réponse corrompue.
  ///
  /// Le filtre porte sur l'objet vide et sur rien d'autre : une entrée non vide que
  /// `JeuInfo` refuse est une vraie divergence de schéma, et doit continuer à faire
  /// du bruit plutôt que de disparaître d'une liste de résultats.
  #[serde(deserialize_with = "jeux_sans_entrees_vides")]
  jeux: Vec<JeuInfo>,
}

fn jeux_sans_entrees_vides<'de, D>(d: D) -> Result<Vec<JeuInfo>, D::Error>
where
  D: serde::Deserializer<'de>,
{
  use serde::de::Error as _;
  let brut = Vec::<serde_json::Value>::deserialize(d)?;
  brut
    .into_iter()
    .filter(|v| !matches!(v.as_object(), Some(o) if o.is_empty()))
    .map(|v| serde_json::from_value(v).map_err(D::Error::custom))
    .collect()
}
```

Une recherche sans résultat rend alors `Ok(vec![])`, ce que rompom sait déjà traiter :
modale vide, honnête, avec la saisie d'un game id.

**Ce que ce correctif ne fait pas**, délibérément : avaler n'importe quelle entrée
illisible. Un jeu réel dont un champ a changé de type doit toujours faire échouer l'appel
— sinon une divergence de schéma se traduirait par des candidats qui disparaissent
silencieusement de la liste, ce qui est pire que l'erreur.

## Test à ajouter

`src/api.rs` a déjà des tests de parsing sur des corps littéraux (`HEADER_OK`). Le cas
tient en une fixture :

```rust
/// ScreenScraper dit « rien trouvé » avec une liste d'un objet vide, HTTP 200 et
/// success=true. C'était un `missing field \`id\`` sur tout l'appel, donc un
/// `ApiFailure::Malformed` chez le consommateur — et, pour rompom, une ROM qu'on ne
/// pouvait plus identifier à la main du tout.
#[test]
fn une_recherche_sans_resultat_rend_une_liste_vide() {
  let body = format!(
    r#"{{ "header": {}, "response": {{ "jeux": [{{}}] }} }}"#,
    HEADER_OK
  );
  assert!(parse_jeu_recherche(&body).unwrap().is_empty());
}

/// Et une entrée non vide que `JeuInfo` refuse reste une erreur : c'est une
/// divergence de schéma, pas une convention de l'API.
#[test]
fn une_entree_incomplete_reste_une_erreur() {
  let body = format!(
    r#"{{ "header": {}, "response": {{ "jeux": [{{"romid": "12"}}] }} }}"#,
    HEADER_OK
  );
  assert!(parse_jeu_recherche(&body).is_err());
}
```

## Comment reproduire sans la lib

`GET https://www.screenscraper.fr/api2/jeuRecherche.php` avec `output=json`,
`systemeid=3`, `recherche=Zzqqxx Nonexistent Game 9999`, plus les quatre paramètres
d'authentification. **Ne jamais recopier l'URL construite dans un log ou un ticket** :
`devpassword` et `sspassword` y sont en clair.

## Au retour

Taguer une **v0.8.1** (correctif de compatibilité, pas de rupture d'API), puis côté
rompom :

1. `tag = "v0.8.1"` dans `Cargo.toml`
2. `nix build` → recopier le `got:` dans `cargoLock.outputHashes` de
   `packages/rompom/default.nix`
3. Rejouer un run `nes` : les trois multicarts doivent arriver dans la vue « à identifier »
   (`m`) au lieu de la vue erreurs.
