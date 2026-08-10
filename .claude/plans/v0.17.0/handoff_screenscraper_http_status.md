# Handoff → agent `screenscraper` : exposer le statut HTTP des erreurs API (v0.7.0)

**Demandeur** : rompom, point P1.1 de sa roadmap (`TODO.md`), lot v0.17.
**Dépôt cible** : `../screenscraper` (actuellement v0.6.0, tag `v0.6.0`).
**Nature** : évolution de l'API publique d'erreur. Aucune modification du parsing JSON,
aucune montée de dépendance.

---

## 1. Pourquoi rompom demande ça

rompom appelle `jeuinfo()` pour identifier chaque ROM par checksum. Quand l'appel échoue,
il ne peut pas savoir **pourquoi** :

```rust
// rompom — src/worker/handlers/discovery.rs:249-256
let ji = if let Some(gid) = cached_game_id {
  ctx.ss.jeuinfo_by_gameid(ctx.system.id, gid).ok()
} else {
  ctx.ss.jeuinfo(ctx.system.id, &filename, size, crc32, md5, sha1).ok()
};
```

Ce `.ok()` écrase toutes les causes en un seul `None`, que rompom interprète comme
« jeu inconnu de ScreenScraper ». Conséquences concrètes :

- un timeout, un 429 (trop de threads) ou un 430 (quota journalier dépassé) ouvrent une
  **modale d'identification manuelle** au milieu du scrape, pour une ROM que SS connaît
  parfaitement ;
- une fois le quota atteint, *toutes* les ROMs restantes défilent en modale ;
- le mécanisme de retry de rompom sur le step `LookupSS` est **du code mort** : l'erreur
  n'est jamais remontée, donc jamais retentée.

rompom ne peut pas corriger ça seul : l'information n'existe pas dans ce qu'il reçoit.

## 2. Le point exact où l'information est perdue

`src/api.rs:162-169` :

```rust
fn get(client: &reqwest::blocking::Client, url: &str, query: &[(&str, String)]) -> Result<String> {
  client
    .get(url)
    .query(query)
    .send()
    .and_then(|r| r.text())   // ← le statut HTTP est jeté ici
    .context(RequestSnafu { url })
}
```

Pas de `error_for_status()`, pas de lecture de `response.status()`. Le corps d'une réponse
d'erreur SS n'étant pas du JSON, il part ensuite dans `serde_json::from_str` et ressort en
`Error::Parse`. **Un 404 « jeu non trouvé » et un 430 « quota dépassé » produisent
aujourd'hui littéralement le même objet d'erreur.**

L'énumération actuelle (`src/api.rs:52-62`) :

```rust
pub enum Error {
  Request { url: String, source: reqwest::Error },  // erreur de transport uniquement
  Parse   { source: serde_json::Error },            // reçoit aussi tous les 4xx
  Api     { message: String },                      // header.success == "false"
}
```

## 3. Ce que dit la doc SS

L'API signale ses erreurs par **code HTTP**, pas dans le corps JSON. Table complète
relevée dans `apiv2.html` (section « Les requêtes de l'API renvoi des numeros d'erreurs
HTTP en cas de problème ») :

| Code | Signification | Nature |
|---|---|---|
| 400 | url sans information / champ obligatoire manquant / nom de rom contenant un chemin (`!mnt!sda1!...`) / champ crc, md5 ou sha1 mal formaté / nom de rom non conforme | bug appelant |
| 401 | API fermée aux non-membres ou membres inactifs — **serveur saturé (CPU > 60 %)** | transitoire |
| 403 | Erreur de login : identifiants développeur erronés | fatal |
| 404 | **Jeu non trouvé / Rom·Iso·Dossier non trouvée** — aucune concordance sur la rom | *le seul cas légitime de modale* |
| 423 | API totalement fermée, graves problèmes serveur | fatal (ce run) |
| 426 | Logiciel de scrape blacklisté (non conforme / version obsolète) | fatal |
| 429 | Nombre de threads autorisés atteint (4 variantes : par membre, par minute, leechers, global) | transitoire, backoff |
| 430 | Quota de scrape journalier dépassé | fatal (ce run) |
| 431 | Trop de roms non reconnues aujourd'hui — « faites du tri et repassez demain » | fatal (ce run) |

## 4. Changement demandé

### 4.1 Capturer le statut dans `get()`

```rust
fn get(client: &reqwest::blocking::Client, url: &str, query: &[(&str, String)]) -> Result<String> {
  let response = client.get(url).query(query).send().context(RequestSnafu { url })?;
  let status = response.status();
  let body = response.text().context(RequestSnafu { url })?;
  ensure!(
    status.is_success(),
    HttpSnafu { url, status: status.as_u16(), body: truncated(&body) }
  );
  Ok(body)
}
```

et une variante correspondante dans `api::Error` :

```rust
#[snafu(display("API returned HTTP {} for {}: {}", status, url, body))]
Http { url: String, status: u16, body: String },
```

> ⚠️ **Piège credentials — à ne surtout pas rater.**
> `base_query()` (`src/lib.rs:144-153`) met `devpassword` et `sspassword` en paramètres
> d'URL. Le champ `url` actuel est sûr **parce qu'il vaut l'URL d'endpoint nue**, avant que
> reqwest n'y greffe la query. Ne **jamais** le remplacer par `response.url()` ni par
> `request.url()` : ça ferait entrer les deux mots de passe dans chaque message d'erreur,
> et donc dans les logs de rompom. Même prudence sur `body` : le tronquer (200 caractères
> suffisent) et ne pas y refléter la requête.

### 4.2 Exposer une classification sur l'erreur **publique**

`mod api;` est privé (`src/lib.rs:1`) : un consommateur ne peut pas nommer `api::Error`,
donc pas matcher dessus. Il faut donc un accesseur sur `screenscraper::Error`, qui traverse
la chaîne `UserInfoFailed`/`JeuInfoFailed`/`SystemsListeFailed`/`JeuRechercheFailed` →
`source`.

Préférence de rompom : une **classification sémantique**, pas un `Option<u16>` brut —
sinon chaque consommateur ré-encode la table de la §3, avec ses propres bugs.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiFailure {
  NotFound,          // 404 — la seule qui justifie une identification manuelle
  BadRequest,        // 400
  ServerBusy,        // 401
  BadDevCredentials, // 403
  ApiClosed,         // 423
  Blacklisted,       // 426
  ThreadLimit,       // 429
  QuotaExceeded,     // 430
  KoQuotaExceeded,   // 431
  Http(u16),         // tout autre statut non-2xx
  Transport,         // Error::Request — timeout, DNS, connexion refusée
  Malformed,         // Error::Parse — réponse 2xx illisible
  Api,               // header.success == "false"
}

impl Error {
  pub fn failure(&self) -> ApiFailure;
  /// `true` pour ce qui a une chance d'aboutir en réessayant : ThreadLimit, ServerBusy, Transport.
  pub fn is_retryable(&self) -> bool;
  /// Raccourci de lisibilité pour `failure() == ApiFailure::NotFound`.
  pub fn is_not_found(&self) -> bool;
}
```

Le cœur doit être une fonction **pure** `fn classify(status: u16) -> ApiFailure`, pour être
testable sans réseau.

`is_retryable()` : `ThreadLimit`, `ServerBusy`, `Transport` → `true`. Tout le reste →
`false`. En particulier `QuotaExceeded`, `ApiClosed` et `KoQuotaExceeded` sont vrais pour
tout le reste du run : les réessayer ne fait que brûler du quota d'erreurs (cf. 431).

### 4.3 Tests attendus (aucun réseau)

- `classify()` : un cas par code de la table §3, plus un statut inconnu → `Http(n)`.
- `is_retryable()` sur chaque variante.
- `failure()` sur une `Error` publique construite à la main pour chacun des quatre
  wrappers, pour vérifier que la traversée `source` fonctionne.
- Un test sur le `Display` d'un `Http` : le message contient l'endpoint et **aucun**
  `devpassword`/`sspassword`.
- Les tests de parsing existants (`api.rs`, 12 tests) doivent passer inchangés.

## 5. Hors périmètre — merci de ne pas le faire ici

- **Politique de retry** : elle appartient à rompom (`execute_step` a déjà son retry avec
  backoff). La lib classe, elle ne réessaie pas et ne dort pas.
- **Montée de `reqwest`** : le pin `0.11.18` est partagé avec `internetarchive` et rompom ;
  en bouger un seul casse le build. C'est une migration coordonnée à trois dépôts, traitée
  séparément (P1.4/P3 côté rompom, avec la sortie de `rustls 0.21`).
- **Parsing JSON, structures `JeuInfo`/`UserInfo`/`System`** : inchangés.
- Observation gratuite, à ignorer pour cette version : `api.rs` tape sur
  `https://www.screenscraper.fr/api2/` alors que `CLAUDE.md` documente
  `https://api.screenscraper.fr/api2/`. Les deux répondent ; à trancher un autre jour.

## 6. Livraison

Le dépôt `screenscraper` n'a ni `Justfile` ni git-cliff : son changelog est **écrit à la
main** dans `README.md` (`## Changelog`, section la plus récente en tête), et une release
est un commit `docs(all): add vX.Y.Z` suivi du tag — cf. `f5d9d25` pour v0.6.0.

1. `Cargo.toml` : `version = "0.7.0"`.
2. `README.md` : section `### 0.7.0` en tête du changelog, décrivant `ApiFailure`,
   `Error::failure()` / `is_retryable()` / `is_not_found()` et la nouvelle variante `Http`.
3. `CLAUDE.md` : compléter « API notes » — les erreurs SS passent par le **statut HTTP**,
   pas par `header.success` ; et rappeler l'interdiction de faire entrer l'URL complète
   (avec query) dans un message d'erreur.
4. Portes : `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, `nix flake check`.
   (Le test d'intégration `it_works` reste `#[ignore]`.)
5. Commits atomiques, Conventional Commits, scope réel. **Ni merge, ni push, ni tag** :
   c'est le mainteneur qui relit, merge et tague `v0.7.0`.

## 7. Ce que rompom fera ensuite (pour information, pas à faire ici)

1. `Cargo.toml` : `tag = "v0.7.0"` sur la dépendance `screenscraper`.
2. `cargo update -p screenscraper` → `Cargo.lock`.
3. `nix build`, récupérer le hash `got:` et le reporter dans `cargoLock.outputHashes` de
   `packages/rompom/default.nix` (obligatoire, sinon le build statique casse).
4. `discovery.rs` : remplacer les `.ok()` / `.unwrap_or_default()` (lignes 249-256,
   277-278, 373, 393) par un match sur `failure()` — `NotFound` seul ouvre la modale,
   `is_retryable()` remonte l'erreur pour laisser le retry du step opérer, le reste échoue
   la ROM avec sa cause (qui sera affichée grâce à P1.2).
