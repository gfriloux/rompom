# Plan v0.17.0 — « confiance » : dire la vérité sur les échecs

## Contexte

v0.16.0 a clos le lot P0 : rompom ne corrompt plus ses paquets ni son état quand quelque
chose casse. Il reste qu'il **ne dit pas** ce qui a cassé, et qu'il se trompe sur la
nature de l'échec.

Deux symptômes, une même cause — l'information d'erreur est jetée avant d'arriver là où
elle sert :

1. `handle_lookup_ss` fait `.ok()` sur l'appel ScreenScraper. Un timeout, un 429 ou un
   quota dépassé deviennent « jeu inconnu », donc une **modale d'identification manuelle**
   injustifiée. Une fois le quota atteint, toutes les ROMs restantes défilent en modale.
   Et le `max_retries = 3` de `LookupSS` est du code mort : l'erreur n'est jamais remontée.
2. `StepStatus::Failed(msg)` porte une cause, mais `bar.finish_error()` ne la prend pas.
   L'utilisateur voit `✗ Jeu` et un compteur d'erreurs, jamais le pourquoi.

Le point 1 était bloqué par la lib `screenscraper`, qui écrasait tous les codes HTTP en
`Error::Parse`. **Débloqué** : la v0.7.0 expose `ApiFailure`, `failure()`, `is_retryable()`
et `is_not_found()` (cf. `handoff_screenscraper_http_status.md` et `phase0_results.md`).

## Objectif

À la fin de v0.17, un échec est *classé* (transitoire / définitif / introuvable), *réessayé
seulement si ça a un sens*, et *affiché avec sa cause* — dans le panneau Completed et dans
le bilan de fin.

## Périmètre

**In scope** — P1.1, P1.2, P1.3, P1.7, P1.8, plus le refactor `StepError` dont P1.1 dépend.

**Out of scope**, avec justification :

- **P1.4 (pinning `Cargo.toml` + dette d'audit)** — sorti du lot. Le pinning des 10 `*`
  fige un `Cargo.lock` que la sortie de `rustls 0.21` va rouvrir immédiatement, et cette
  sortie impose de bouger rompom + screenscraper + internetarchive ensemble. Les deux
  méritent leur propre lot, avec `nix build` et `just audit` comme portes. Le seul bout
  gratuit ici (retirer la section `[target.x86_64-unknown-linux-gnu]` invalide, qui produit
  un `unused manifest key` à chaque build) part en phase 1, où l'on touche déjà `Cargo.toml`.
- **P2.x** — UX, lot v0.18.
- **Contribution SS** — toujours bloquée, endpoint non documenté (cf. `TODO.md` P3).

## Étages concernés

`pipeline` (steps, worker, handlers) · `ui` (panneau + summary) · `conf` · `package` · `nix`

## Décision technique structurante : `enum StepError`

P1.1 a besoin de distinguer « réessaie » de « abandonne tout de suite ». Aujourd'hui
`execute_step` retente **toute** `Err(String)` jusqu'à `max_retries`, sauf la sentinelle
`Err("interrupted")` reconnue par comparaison de chaîne. Sur un quota journalier dépassé,
ça brûle 3 tentatives et 7 s de backoff par ROM pour rien — et le 431 SS punit justement
l'excès de requêtes ratées.

On introduit donc, **avant** P1.1 :

```rust
pub enum StepError {
  Interrupted,        // Ctrl-C pendant l'acquisition d'un sémaphore
  Transient(String),  // vaut la peine d'être réessayé (réseau, 429, 401)
  Fatal(String),      // définitif : quota, blacklist, identifiants, bug
}
```

C'est un item listé en P3, remonté ici parce que P1.1 et P1.2 en dépendent tous les deux :
P1.1 pour la décision de retry, P1.2 pour le message à afficher. Le faire d'abord évite
d'écrire deux fois la même logique en sentinelles de chaîne.

## Phases

Chaque phase passe `just ci` seule et fait un commit.

### Phase 1 — passer à screenscraper v0.7.0

- `Cargo.toml` : `tag = "v0.7.0"` ; retirer au passage la section
  `[target.x86_64-unknown-linux-gnu]` (ignorée par cargo, bruit à chaque build).
- `cargo update -p screenscraper` → `Cargo.lock`.
- `packages/rompom/default.nix` : renommer la clé `screenscraper-0.6.0` → `-0.7.0` et
  y mettre le hash rendu par `nix build` (`got:`).
- **Vérification** : `just ci` **et** `nix build` (le second est la seule porte qui teste
  vraiment `outputHashes`).
- Commit : `chore(deps): bump screenscraper to v0.7.0`

### Phase 2 — `StepError`

- `src/rom/step.rs` : l'enum + `Display`.
- `src/worker/mod.rs` : les handlers renvoient `Result<StepStatus, StepError>` ;
  `execute_step` route `Interrupted` → `Pending` sans dispatch (comportement actuel),
  `Transient` → retry si `retry_count < max_retries`, `Fatal` → `Failed` immédiat.
  Une panique reste `Fatal`.
- Les 8 handlers : `Err("…".to_string())` → `StepError::Transient(…)` par défaut,
  `Err("interrupted")` → `StepError::Interrupted`. Mécanique, aucun changement de
  comportement attendu à ce stade.
- **Tests** : un `Fatal` ne réessaie pas ; un `Transient` réessaie jusqu'à `max_retries`
  puis devient `Failed` ; `Interrupted` laisse le step `Pending` et ne décrémente aucun
  `wait_for`.
- Commit : `refactor(worker): type step failures instead of string sentinels`

### Phase 3 — P1.1 : croire ScreenScraper sur parole

`src/worker/handlers/discovery.rs` :

- `jeuinfo` / `jeuinfo_by_gameid` (l. 249-256) : `is_not_found()` → modale (seul cas
  légitime) ; `is_retryable()` → `Transient` ; le reste → `Fatal` avec la cause.
- `jeu_recherche` (l. 275-278) : `unwrap_or_default()` masque un échec en « aucun
  candidat » et ouvre une modale vide. Même traitement.
- `fetch_by_id` (l. 366-380) : la closure de la modale renvoie `Option<String>`, donc
  « ID inconnu » et « réseau coupé » s'affichent pareil. Passer à `Result<String, String>`
  pour que le message inline dise lequel des deux. Touche `ui/modal.rs` et
  `ui/mod.rs` (`ModalRequest::fetch_by_id`).
- `main.rs` (l. 562) : à la connexion, distinguer identifiants erronés (403) d'un souci
  réseau. **Attention** : ne jamais faire figurer les identifiants dans le message.
- **Tests** : la table de correspondance `ApiFailure → StepError` est extraite en fonction
  pure et testée variante par variante, sans réseau.
- Commits : `fix(discovery): stop treating a ScreenScraper outage as an unknown game`,
  puis `feat(modal): tell a wrong game ID from a network failure`.

### Phase 4 — P1.2 : afficher la cause d'un échec

- `ui/mod.rs` : `CompletedEntry.error: Option<String>` ; `finish_error(&self, cause: &str)`.
- Appelants : `worker/mod.rs:224` (a `msg` sous la main) et `worker/run_state.rs:167`
  (a le `StepStatus::Failed(msg)` restauré).
- `ui/render.rs` : ligne `✗ Jeu — cause`, tronquée à la largeur du panneau.
- `summary.rs` : `failures: Vec<(String, String)>` et une section « Failures » dans
  `print()`, non tronquée — c'est le seul endroit où la cause complète survit à la fin du
  run.
- **Tests** : `Summary` construit depuis un `AppState` porteur d'échecs ; troncature.
- Commit : `feat(ui): show why a ROM failed, in the panel and in the summary`

### Phase 5 — P1.3 : messages d'erreur de configuration

`conf/mod.rs:99-113` : `ReadConfiguration` et `ParseConfiguration` n'ont pas de
`#[snafu(display)]`, donc le chemin du fichier et la position ligne/colonne de `serde_yaml`
sont perdus au profit du nom de variante. Les ajouter (`WriteConfiguration` aussi).
`ParseConfiguration` ne porte même pas le `path` : l'ajouter.

Commit : `fix(conf): name the file and the line when the configuration is unreadable`

### Phase 6 — P1.7 : `./launcher` OpenBOR écrit dans le CWD

`package.rs:181` écrit `./launcher` dans le répertoire courant, partagé par tous les
workers : deux ROMs OpenBOR packagées en parallèle s'écrasent l'une l'autre. L'écrire dans
le répertoire de la ROM, comme tout le reste.

Commit : `fix(package): write the OpenBOR launcher into the ROM directory`

### Phase 7 — P1.8 : durabilité de l'état

- Flush périodique de `state.yml` (aujourd'hui écrit une seule fois, après le join des
  workers : un crash — à distinguer d'un Ctrl-C, qui lui sauve — perd tout et re-bumpe
  tous les `pkgver` au run suivant).
- `run.yml` : write-rename, comme `state.yml` le fait déjà.
- `state.rs:29-31` : un `state.yml` illisible est ignoré **en silence**, ce qui ressemble
  à un premier run et re-télécharge tout. Avertir.
- **Tests** : round-trip `SystemState` ; le flush périodique ne perd pas d'entrée.
- Commit : `fix(state): survive a crash without losing the run's state`

## Ordre et dépendances

```
Phase 1 (bump lib) ──→ Phase 3 (P1.1)
Phase 2 (StepError) ─┘        │
                              └──→ Phase 4 (P1.2)
Phases 5, 6, 7 — indépendantes, dans cet ordre par coût croissant
```

Phases 5 à 7 peuvent être livrées séparément si le lot devient trop gros : v0.17 reste
cohérente avec les phases 1 à 4 seules (« un échec est classé, réessayé à bon escient et
affiché »).

## Risques

- **Le hash Nix.** Oublier `outputHashes` casse `build-static` sans casser `just ci` :
  `nix build` est une porte explicite de la phase 1.
- **Ratisser trop large en phase 2.** Le refactor touche les 8 handlers ; il doit rester
  strictement mécanique, sans changement de comportement, sinon la régression est
  indétectable au milieu de P1.1.
- **Modale non testable automatiquement.** Les cas 429/430 se vérifient à la main
  (cf. `manual_tests.md`).

## Fichiers touchés

`Cargo.toml` · `Cargo.lock` · `packages/rompom/default.nix` · `src/rom/step.rs` ·
`src/worker/mod.rs` · `src/worker/handlers/*.rs` · `src/worker/run_state.rs` ·
`src/ui/mod.rs` · `src/ui/render.rs` · `src/ui/modal.rs` · `src/summary.rs` ·
`src/conf/mod.rs` · `src/package.rs` · `src/state.rs` · `src/main.rs` · `CLAUDE.md`

`CLAUDE.md` est mis à jour **dans le commit de la phase concernée** : `StepError` remplace
la sentinelle décrite dans « Ctrl-C handling », et la section « Failure propagation »
gagne la distinction transitoire/définitif.
