# TODO — Roadmap qualité & simplicité

Issu de la revue complète du 2026-08-08 (sécurité, concurrence, UX/CLI, outillage).

Diagnostic transverse : le chemin nominal est soigné, mais les chemins d'échec
(panique, step Failed, resume partiel, données réseau hostiles) ne sont pas traités
comme des chemins de première classe. C'est là que se concentrent les problèmes graves.

Points sains à préserver : architecture DAG + TaskQueue, extraction sous lock avant
I/O réseau, write-rename du state.yml, code clippy-clean, Cargo.lock commité,
TLS correct, pas de fuite de credentials, XML échappé via quick-xml.

---

## P0 — Corruption / sécurité — **CLOS le 2026-08-09 (v0.16.0)**

Les six points sont corrigés. Le détail vit dans `.claude/plans/v0.16.0/`. Chaque
correction est arrivée avec ses tests : le dépôt est passé de 0 à 34 tests.

- [x] **P0.1 — Injection shell dans les PKGBUILD.** Le trou était plus large que décrit :
  `pkgdesc` recevait le nom ScreenScraper **totalement brut**, sans même la liste noire.
  Rejouée sur des noms hostiles, l'ancienne `normalize_name()` laissait passer backtick,
  `"`, `\`, saut de ligne, `|` et `>` — et `_romname="…"` étant entre guillemets doubles,
  le backtick y est une substitution de commande. Corrigé par `shell_quote()` (la valeur
  porte ses propres quotes, les templates interpolent nu), liste blanche sur
  `normalize_name()`, `sanitize_token()` sur format/région, `sanitize_sha1()` qui échoue
  fermé. Non prévus au diagnostic : repli ASCII des accents (« Astérix » → `asterix`),
  repli sur le hash pour les titres non latins (tous les jeux japonais partageaient sinon
  un seul `pkgname`), et `sed_pattern()` pour le `sed` Sega CD — ce qui clôt au passage
  le point `|` listé en P3.
- [x] **P0.2 — Path traversal via `m.format`.** Un format `png/../../x` écrivait dans
  `/out/roms/x`, deux niveaux au-dessus. Le piège réel n'était pas la fonction mais la
  **cohérence** : `package.rs` assainissait déjà le format côté PKGBUILD et
  `downloads.rs` calculait son nom de son côté. D'où `media_ext()`, partagée, avec repli
  `bin` — sinon `makepkg` cherche un fichier jamais écrit.
- [x] **P0.3 — Panique de handler = deadlock global.** Deux découvertes : le profil Nix
  posait `panic = "abort"`, qui rendait `catch_unwind` inopérant dans le binaire livré
  (retiré, coût mesuré +483 Ko / +4,3 %) ; et attraper la panique ne suffit pas, il faut
  `clear_poison()` sur les trois mutex, sans quoi le verrou suivant panique et le worker
  meurt quand même. Les paniques contournent aussi le retry — un dépassement d'index
  retombe à l'identique.
- [x] **P0.4 — Step `Failed` corrompt l'état.** Préalable indispensable : déplacer la
  décrémentation de `remaining` hors de `handle_save_state` vers `finish_rom` dans
  `execute_step`. Sans ça, couper les successeurs réintroduisait le blocage de P0.3.
  La propagation suit un ensemble de visités et non le statut, parce que `WaitModal`
  démarre `Skipped` : un garde par statut se serait arrêté net dessus.
- [x] **P0.5 — Resume corrompt les paquets.** Le remède prévu (« remettre LookupSS à
  Pending ») était insuffisant : **chaque** step alimente les suivants en mémoire
  (`sha1`, `jeu`, `medias`, `romname`…), aucun préfixe du pipeline n'est fiable. Le
  resume est devenu **tout ou rien par ROM** — abordable parce que tout ce qui coûte est
  déjà idempotent. Un jeu identifié à la main via la modale doit l'être à nouveau.
- [x] **P0.6 — `unwrap()` de la collecte paniquent sous TUI.** `collect_sources()`
  remonte l'erreur en nommant l'item, le motif ou le chemin fautif ; le `Ui` est relâché
  avant l'écriture. Au passage : globs compilés une fois par système au lieu d'une fois
  par fichier candidat, et noms de fichiers non-UTF-8 ignorés plutôt qu'`unwrap()`és.
  La date SS était pire que « parfois invalide » — valider par longueur de chaîne n'est
  pas valider : `"abcd"` fait 4 caractères, `"2024-13-45"` en fait 10.

## P1 — Robustesse et confiance

- [x] **P1.1 — Erreur réseau SS ≠ « jeu non trouvé »** — *fait le 2026-08-10*, après la
  sortie de `screenscraper` v0.7.0. `lookup_failure()` traduit `ApiFailure` en
  `StepError` : seul le 404 ouvre la modale. Trois trous non prévus au diagnostic, tous
  trouvés en lisant le code : `jeu_recherche(...).unwrap_or_default()` ouvrait une modale
  **vide** sur échec réseau ; le `jeuinfo_by_gameid` d'après-modale faisait `.ok()`, donc
  une coupure à cet instant jetait l'identification que l'utilisateur venait de saisir et
  écrasait le `description.xml` par un vide ; et un Ctrl-C dans la même fenêtre tombait
  dans le même trou via un `return None`.
  - **Fuite de credentials trouvée au passage** (corrigée) : `reqwest::Error` ajoute
    ` for url (<url complète>)` à son `Display` (reqwest-0.11.27, `src/error.rs:205`) et
    `base_query()` passe `devpassword`/`sspassword` en paramètres d'URL. `main.rs`
    imprimait cette erreur telle quelle : perdre le réseau au démarrage écrivait les deux
    mots de passe sur stderr. rompom ne cite plus jamais l'erreur de la lib — il compose
    sa propre phrase à partir de `ApiFailure`, et un test le vérifie.
  - Ancien diagnostic, conservé pour mémoire :
  - **Bloqué par la lib `screenscraper`** (constaté le 2026-08-09, avant de coder) :
    l'API ScreenScraper signale ses erreurs par **code HTTP** — `404` jeu introuvable,
    `429` trop de threads, `430` quota journalier, `423` API fermée, `403` identifiants
    (cf. `../screenscraper/apiv2.html`). Or `api::get()` fait
    `.send().and_then(|r| r.text())` **sans `error_for_status()`** : le statut est
    jeté, et comme le corps de ces réponses n'est pas du JSON, tout revient
    indistinctement en `Error::Parse`. Un `404` et un `430` sont donc littéralement le
    même objet d'erreur côté rompom.
  - **Correction complète** : ajouter dans la lib une variante portant le statut
    (~15 lignes dans `get()`), taguer une v0.7.0, puis mapper `404` → modale et tout le
    reste → retry. Impose de mettre à jour le `tag =` dans `Cargo.toml` **et** le
    `cargoLock.outputHashes` du paquet Nix. *(décision à prendre : touche un second dépôt)*
  - **Fix partiel possible sans la lib** : traiter `Error::Request` (timeout, DNS,
    connexion refusée) comme transitoire. Ne couvre que le timeout des trois cas cités.
- [x] **P1.2 — Afficher les erreurs des ROMs échouées** — *fait le 2026-08-10*.
  Panneau Completed `✗ rom — cause` (tronquée à la largeur, à la place des icônes médias)
  et section `Failures` non tronquée dans `Summary::print()`. Trouvé en chemin :
  `restore_bar_for_resumed_rom()` ne lisait que la feuille du pipeline, or une ROM coupée
  en amont a `Skipped` partout après le step cassé — feuille comprise. Un échec repris
  depuis `run.yml` réapparaissait donc en **succès**.
- [x] **P1.3 — Messages d'erreur config** — *fait le 2026-08-10*. `#[snafu(display)]` sur
  les quatre variantes, `path` ajouté à `ParseConfiguration`. Une erreur YAML donne
  maintenant le fichier, le champ et la ligne. Au passage : `--update-config` rapportait
  ses échecs de **sérialisation** sous `ParseConfiguration` — d'où une variante
  `SerializeConfiguration` distincte.
- [x] **P1.4 — Cargo.toml** — *fait le 2026-08-10*. Les dix `*` pinnés sur les valeurs que
  `Cargo.lock` résolvait déjà (lock inchangé, c'était la vérification) ; `serde_derive`
  retiré, ce qui imposait aussi de basculer deux `use` sur `serde::` ; `[profile.release]`
  déplacé du bloc `env` du Nix vers `Cargo.toml`, **avec** le commentaire sur
  `panic = "abort"` — les variables `CARGO_PROFILE_RELEASE_*` écrasent le manifeste, garder
  les deux garantissait une divergence qu'aucune porte ne détecte. Vérifié par `nix build`
  avant/après : 11 609 000 octets les deux fois.
  - **`checksums` abandonnée** au passage (`src/hash.rs` : `sha1`, `md-5`, `crc32fast`).
    C'était la seule dette d'audit que rompom pouvait solder seul, et un test temporaire
    a comparé les deux implémentations sur de vrais fichiers — padding CRC32 compris —
    avant de retirer la crate. Résultat : `RUSTSEC-2022-0004` sort de `.cargo/audit.toml`,
    les avertissements passent de 9 à 4, et le binaire perd 197 Ko.
  - **Reste la dette rustls**, inchangée et toujours ignorée avec justification :
    `rustls-webpki 0.101.7` — 3 advisories (RUSTSEC-2026-0098/0099/0104), via
    `rustls 0.21` ← `reqwest 0.11`. Sortie = reqwest 0.12, **impossible pour rompom
    seul** : screenscraper et internetarchive pinnent aussi reqwest 0.11, les trois
    doivent bouger ensemble. Se combine avec la migration rustls de P3. *(moyen)*
- [x] **P1.5 — CI GitHub Actions** — *fait le 2026-08-09*. `.github/workflows/ci.yml` :
  job `ci` (`just ci` = version-check + fmt-check + clippy `-D warnings` + test),
  job `nix` (`nix flake check`), job `audit` (`cargo audit`, advisory). Le `Justfile` est
  la seule définition des portes ; pre-commit l'appelle aussi. Voir `PROCEDURE_PLANS.md` §7.
- [x] **P1.6 — Premiers tests unitaires** (fonctions pures, sans réseau) — *fait le
  2026-08-10*. Le dépôt passe de 59 à 89 tests. `disc_indicator()` et
  `group_multi_disc()` sont partis dans `src/collect.rs` avec leurs dix tests ;
  `search_name()`, `check_media_changes()`, `apply_game_path()` et `read_pkgver()` ont
  les leurs. **Ce point réclamait deux tests qui existaient déjà** : le round-trip
  `SystemState` (`state::tests::a_state_survives_a_round_trip`) et `apply_run_state()`
  (sept tests dans `worker::run_state::tests`), écrits en v0.16/v0.17 sans que la
  roadmap soit mise à jour.
- [x] **P1.7 — `./launcher` OpenBOR écrit en CWD** — *fait le 2026-08-10*. Écrit
  maintenant dans le répertoire de la ROM.
  - **À creuser (bug distinct, non corrigé)** : ce fichier `launcher` n'est référencé
    **nulle part** — ni dans les `sources` du PKGBUILD, ni dans un `package()`. Et
    `apply_game_path()` pose `game.path = ./{name}.sh` pour le système 214, donc
    EmulationStation cherche un `.sh` que rien ne produit. Le packaging OpenBOR est
    probablement cassé de bout en bout ; à instruire avec un vrai jeu OpenBOR avant de
    décider quoi corriger. *(moyen)*
- [x] **P1.8 — Durabilité de l'état** — *fait le 2026-08-10*. Flush toutes les 30 s par
  un thread dédié (sérialisation sous le verrou, écriture hors verrou) ; `run.yml` passe
  par le même `write_with_rotation()` que `state.yml` ; `SystemState::load()` rend un
  avertissement, imprimé **avant** `Ui::new()` pour qu'il soit lisible.

## P2 — Simplicité d'utilisation

- [x] **P2.1 — CLI** — *fait le 2026-08-10*. `panic!` remplacé par le message de `getopts`
  + usage + exit 2 ; `--version` (répondu avant toute lecture disque, donc utilisable sur
  une machine sans config) ; `--list-systems` ; codes de sortie **0** le run a eu lieu,
  **1** il n'a pas pu démarrer, **2** la ligne de commande est fautive — documentés dans
  le README. Système inconnu et système sans `source` sortaient en 0, donc en CI un nom
  mal orthographié était un build vert qui n'avait rien scrapé.
- [x] **P2.2 — `rompom --init`** — *fait le 2026-08-10*. Écrit le sample de la racine
  (`include_str!`) avec `create_new` et non `exists()` puis write : ce fichier porte le
  mot de passe ScreenScraper, le refus n'a de valeur que si le test et l'écriture sont
  la même opération. `lang` est validé contre `SUPPORTED_LANGS` (casse repliée), avec un
  message listant les six codes qui marchent — un `lang: [fr-FR]` chargeait sans broncher
  et rendait tous les synopsis vides pour le run entier.
- [x] **P2.3 — Statut `retrying (2/3)...`** — *fait le 2026-08-10*. Posé **avant** le
  `sleep` du backoff, en jaune : tout l'intérêt est que la barre ait quelque chose à
  montrer pendant que le worker attend.
- [x] **P2.4 — Multi-disc, cas limites** — *fait le 2026-08-10*, trois paires
  test-rouge + correctif. Le nom de groupe est désormais le stem **privé du seul groupe
  disque**, donc un tag de part et d'autre survit ; deux fichiers réclamant le même
  numéro font **refuser** le groupe (deux paquets se réparent à la main, un `.m3u` faux
  ne se voit qu'à la manette) ; et `MAX_DISC = 20` écarte `(CD32)` et `(CD64)` sans
  perdre la forme courte `(CD1)`, ce qu'aurait fait la règle du séparateur.
- [x] **P2.5 — Documentation** — *fait le 2026-08-10*. Nerd Font en prérequis (+
  `--ascii`), tableau des trois fichiers écrits **dans le répertoire courant**, ce que
  coûte la suppression de `state.yml` (toute la bibliothèque retéléchargée, chaque
  `pkgver` bumpé, donc republiée à qui suit le dépôt), et le lien tier SS ↔ `maxthreads`
  ↔ vitesse d'identification.
- [x] **P2.6 — Mode non-interactif `--plain`** — *fait le 2026-08-10*. Les trois bloqueurs
  sont tombés : le prompt resume (`--resume=yes|no`, et EOF vaut **non** — `read_line`
  rend `Ok(0)` sur stdin fermé et la réponse vide passait pour le *oui* par défaut) ;
  la TUI (`--plain`, impliqué quand stdout n'est pas un terminal) ; et la modale
  (`StepError::Fatal` explicite plutôt qu'une identification devinée). **Trouvé en
  chemin** : sans terminal de contrôle, `enable_raw_mode().unwrap()` paniquait sur le
  thread de rendu et `install_panic_hook()` avalait le message — le run allait au bout
  sans rien afficher et sans rien signaler.
- [x] **P2.7 — Refonte TUI « turn 4 »** — *fait le 2026-08-11 (v0.19.0)*. La découpe par
  phase a disparu (`PANELS`, `PanelDef`, `RomPhase`, `render_active()`, `render_panel()`,
  `render_completed()`, `CompletedEntry`) : une ligne par ROM à sa place d'arrivée, pour
  tout le run. Livré : grille (cellules `id`/`pkg`/`rom` + 9 pastilles), sélection avec
  trois lignes de détail, bandeau progression + sparkline + débit + ETA + workers actifs,
  vues filtrées erreurs (`e`) et à identifier (`m`), `w` → `<system>.errors.log`, modal
  réaligné sur les colonnes de la grille, vue repliée < 100 colonnes, bilan de fin dans
  l'écran (`q` pour quitter), palette truecolor + repli 16 couleurs. `--plain`,
  `plain_line()`, `Summary::print()` et le statut `retrying` sont conservés intacts.
  - **Trois demandes de la spec sans source de données**, constatées avant de coder :
    - **`62 %` / débit instantané / `rom 2.4/3.9 MiB`** — ni `internetarchive` ni
      `screenscraper` n'expose la progression d'un téléchargement. Handoffs écrits
      (`.claude/plans/v0.19.0/handoff_*.md`), colonne en spinner en attendant. Le volume
      et le débit **moyen** sont livrés, comptés par fichier terminé.
    - **`meilleur score 91 %` et la touche `a`** — `jeuRecherche` classe par probabilité
      et ne renvoie **aucun** pourcentage ; le champ `score` de l'API SS est une note
      utilisateur sur 20. Remplacés par le **rang** dans le modal et par le **nom du
      premier candidat** dans la vue `m`. Pas d'acceptation en masse : elle se serait
      appuyée sur un chiffre inventé.
    - **`r` / `R`** — descendus en P3 (voir ci-dessous) : c'est du pipeline, pas de l'UI.
  - **Bonne surprise** : `jeu_recherche` renvoie des `JeuInfo` **complets** (l'API est
    « identique à jeuInfos sans les infos ROM »), donc les pastilles médias par candidat
    et la ligne `selection` du modal ne coûtent **aucun** appel supplémentaire. Le
    handoff en budgétait un par candidat et prévenait que ce serait peut-être trop cher.
  - **Trouvé en chemin** : `find_desc()` répond `"Unknown"` et non une chaîne vide quand
    un jeu n'a pas de synopsis — tester la vacuité allumait la pastille description en
    vert pour **tous** les candidats.
  - **Effet de bord** : `modal_sem` supprimé (cf. P3), ce qui ferme la fuite de permit.

## P3 — Dette et long terme

- [x] **Dédupliquer** — *fait le 2026-08-12 (v0.21.0)*. Les quatre points sont soldés :
  `rom_unchanged` devient une fonction pure unique (sept tests), les huit blocs de
  `build_pkgbuild` une boucle, `copy_rom`/`download_rom` un `place_discs()` commun, et la
  liste des médias **une** table. Trois écarts entre le diagnostic et le code réel :
  - la liste des 8 médias était écrite **cinq** fois, pas quatre, et selon **deux ordres
    différents** — `build_pkgbuild` mettait bezel en 2ᵉ, les trois sites du worker en 4ᵉ,
    l'écran en 6ᵉ. `MEDIA_KINDS` unifie sur l'ordre de l'écran, donc les pastilles se
    remplissent maintenant de gauche à droite, et **l'ordre des `sources` du PKGBUILD
    change** (sans risque : `makepkg` apparie par position, la boucle pousse dans les deux
    tableaux d'un coup, et le PKGBUILD n'entre pas dans la décision de bump).
  - la bascule `video-normalized` → `video` était écrite deux fois ; elle vit dans
    `pick_media()`.
  - **bug trouvé en chemin, corrigé** : `make_game()` nommait les chemins de
    `description.xml` avec le `format` **brut** de ScreenScraper, alors que le fichier
    écrit passe par `media_filename()` (liste blanche de P0.2). Sur `png/../../x` le
    fichier s'appelle `thumbnail.pngx` et EmulationStation était envoyé sur
    `thumbnail.png/../../x` — inexistant, et hors du répertoire du jeu.
- [x] ~~Utiliser `m.url` au lieu de reconstruire les URLs SS à la main dans
  `build_pkgbuild`~~ — **À NE PAS FAIRE.** *Constaté le 2026-08-11 en préparant v0.20.0.*
  `m.url` n'est pas un lien CDN public : c'est l'appel `mediaJeu.php` que ScreenScraper
  renvoie, et `base_query()` met `devid`, `devpassword`, `ssid` et `sspassword` dans
  chaque requête. Les URLs du PKGBUILD sont **publiées** — s'en servir écrirait les deux
  mots de passe dans le dépôt de paquets. La reconstruction à la main
  (`https://screenscraper.fr/medias/{systemeid}/{jeuid}/{slug}.{ext}`, où `slug` sort du
  paramètre `media=` de cette même URL) est un **blanchiment délibéré**, pas une
  duplication naïve. `media_url()` porte le commentaire qui l'explique, pour que
  personne ne « simplifie » ça un jour.
- [x] `enum StepError { Interrupted, Transient, Fatal }` au lieu de la sentinelle
  `Err("interrupted")` — *fait le 2026-08-10*, remonté dans le lot v0.17 parce que P1.1
  en dépend : sans la distinction transitoire/définitif, un quota dépassé brûlait 3
  tentatives et 7 s de backoff par ROM. La décision vit dans `disposition()`, pure et
  testée.
- [x] **Code mort** — *fait le 2026-08-11 (v0.20.0)*. `Phase`/`StepKind::phase()` et
  `Package.name` supprimés. `StepData` réduit à `LookupSS { candidates }` : « quasi
  entier » était le mot juste, cette variante-là est réellement lue — `LookupSS` et
  `WaitModal` tournent sur des pools différents et ne peuvent pas se passer les
  candidats directement.
- [x] **Concurrence** — *fait le 2026-08-12 (v0.21.0)*. Les trois points :
  - `debug_assert!` anti-wrap dans `dec_wait_for`. Le prix du bug était pire que « wrap » :
    à `usize::MAX` le test `remaining == 0` de l'appelant ne matche jamais, le successeur
    n'est pas poussé et le run **attend indéfiniment**, sans rien afficher.
  - `cancelled` sous le mutex du sémaphore : l'attente perd son timeout, donc plus de
    réveil 20 fois par seconde par worker en attente. Un permit libre continue de primer
    sur le drapeau — un worker qui peut avancer finit son step.
  - retry sans `thread::sleep` : `TaskQueue::push_after()` porte l'échéance, `pop_main()`
    attend jusqu'à la plus proche. Le worker retourne au pool immédiatement, et surtout
    **Ctrl-C n'attend plus la fin du backoff** (jusqu'à 16 s) : `shutdown()` réveille ce
    qui attend sur quelque chose, et un thread endormi n'attend sur rien. À l'arrêt les
    tâches datées sont abandonnées — elles sont `Pending`, `run.yml` les porte.
  *(la fuite de permit `modal_sem` est close en v0.19 — le sémaphore a été supprimé, pas
  emballé dans un guard : il sérialisait ce que le thread de rendu sérialise déjà, et sa
  capacité de 1 était ce qui empêchait la vue « à identifier » d'avoir quoi que ce soit
  à lister)*
- [ ] **`r` / `R` — relancer les ROMs en échec depuis la TUI** *(descendu de P2.7)*.
  Ce n'est pas de l'UI : il faut ré-armer le DAG (steps `Failed`/`Skipped` remis à
  `Pending`), ré-incrémenter `remaining` sans casser l'invariant anti-underflow, et
  repousser dans la queue. Après la fin du run la queue est arrêtée et les workers
  joints, donc un `R` sur l'écran de bilan demanderait de relancer un pool. Mérite son
  propre plan. En attendant, `w` écrit `<system>.errors.log` et relancer
  `rompom -s <system>` refait exactement les ROMs échoués — leur `state.yml` n'a rien
  enregistré. *(moyen)*
- [x] **Progression par téléchargement** — *fait le 2026-08-11 (v0.20.0)*, après
  `internetarchive` v0.3.0 et `screenscraper` v0.8.0. `Cell::Progress(u8)`,
  `RomBar::rom_progress()`, volume et débit à l'octet. **Le piège n'était pas le
  branchement** : `read` recule deux fois — IA tronque et repart de zéro sur bascule de
  miroir, et le fichier suivant du même ROM repart de zéro aussi. `transfer_delta()` lit
  toute baisse comme « un transfert a commencé », donc le compteur du run reste monotone.
  `CopyRom` reste au spinner : `fs::copy` ne rend jamais la main.
- [x] **Templates** — *fait le 2026-08-11 (v0.20.0)*. `mkdir -m 0700 -p` (l'ancien
  créait un répertoire nommé `0700` et laissait les vrais aux droits par défaut) et la
  virgule parasite de `ls *.pdf,` (qui faisait perdre le manuel silencieusement).
- [ ] **Lib `screenscraper` — assainir le `Display` de `Error::Request`.** La variante
  porte un `source: reqwest::Error` dont le `Display` ajoute l'URL complète, credentials
  compris. rompom est protégé (il ne cite plus l'erreur), mais le prochain consommateur
  ne le saura pas. Correctif amont : `.without_url()` sur l'erreur avant de la stocker
  (reqwest l'expose, `src/error.rs:80`). *(petit, dépôt voisin)*
- [ ] Migration rustls (rompom + screenscraper + internetarchive) → supprime openssl
  vendored + perl du Nix. Remplacer `serde_yaml` (archivé). `Debug` masqué sur
  `Auth`/`ScreenScraper`.
- [ ] Migration clap ; regrouper les fichiers d'état dans `.rompom/` (avec migration) ;
  checks Nix clippy + cargo test + `--edition 2021` sur le check rustfmt.
- [ ] **Contribution SS** — soumettre l'association `checksum → game_id` à ScreenScraper
  quand l'utilisateur identifie une ROM à la main via la modale. Ajoute un step
  `ContributeRomToSS` au DAG entre `WaitModal` et `BuildPackage` (fire-and-forget : un
  échec de contribution ne fait pas échouer le packaging), plus une fonction
  `contribute_rom(...)` dans la lib `screenscraper`. Tout est déjà disponible au retour de
  la modale (sha1/md5/crc32, filename, size, system.id, game_id choisi).
  **Bloqué** : le compteur `romasso` de `UserInfo` prouve que SS supporte la fonction,
  mais la route API n'est pas dans la doc v2 — aucun `modiftypeinfo` de `botProposition.php`
  ne couvre l'association de checksum. Débloquer par le forum/Discord SS, ou en observant
  les requêtes d'un autre scraper. Hors périmètre : contribution de jeu complet, et
  multi-disques (les disques 2+ n'ont pas de game_id distinct).

---

## Séquencement

1. ~~**v0.15.1** — P0 + P1.1-P1.3~~ → devenu **v0.16.0** : l'outillage (Justfile, CI,
   changelog généré, montée quick-xml, premiers tests) avait déjà atterri sur `master`
   sans release, donc le tag contenait des ajouts et pas seulement des correctifs.
2. **v0.16.0 — livrée le 2026-08-09.** Outillage + **P0 complet** + P1.5 (CI) + amorce de
   P1.6 (34 tests). P1.1–P1.3 ont été **sortis du périmètre** en cours de route : le lot
   P0 formait un ensemble cohérent et publiable, et P1.1 s'est révélé bloqué par la lib
   `screenscraper` (cf. ci-dessus).
3. **v0.17.0 — livrée le 2026-08-10.** Confiance : P1.1 (après la sortie de
   `screenscraper` v0.7.0), P1.2, P1.3, P1.7, P1.8, plus `StepError` remonté de P3.
   P1.4 en est **sorti en cours de route** : le pinning n'a rien à voir avec la confiance
   dans les chemins d'échec, et le lot était déjà cohérent sans lui.
4. **v0.18.0 — livrée le 2026-08-10.** Dette puis UX, dans cet ordre : P1.4 (+ sortie de
   `checksums`), P1.6 (89 tests, `src/collect.rs` extrait), P2.4, P2.1, P2.2, P2.3, P2.5,
   P2.6. Les tests de P1.6 sont passés **avant** P2.4 exprès : les deux fonctions que
   P2.4 corrige n'avaient aucune couverture, et le diff des tests montre exactement ce
   que les correctifs ont changé.
5. **v0.19.0 — livrée le 2026-08-11.** Refonte TUI « turn 4 » (P2.7), seule, comme prévu.
   Trois morceaux de la spec sont **sortis du périmètre faute de données** et non par
   manque de temps : le pourcentage par téléchargement (aucune des deux libs ne le
   rapporte), le score de pertinence (ScreenScraper n'en renvoie pas) et la relance
   `r`/`R` (du pipeline, pas de l'UI). Les deux premiers ont un handoff, le troisième un
   point P3.
6. **v0.20.0 — livrée le 2026-08-11.** La progression des téléchargements, débloquée par
   `internetarchive` v0.3.0 et `screenscraper` v0.8.0, plus un lot de P3 : les deux bugs
   de templates, le code mort, `Package.name`. **Trouvé en préparant le lot** : une fuite
   de credentials vivante sur le chemin médias — rompom citait le `Display` de l'erreur
   de la lib, qui contient l'URL `mediaJeu.php` avec les deux mots de passe. Même classe
   de bug que P1.1, sur le chemin que P1.1 n'avait pas audité. Corrigée en tête de lot,
   et l'item « utiliser `m.url` » est passé d'une tâche à un avertissement.
7. **v0.21.0 — le lot dédup + concurrence.** Les deux premiers points ouverts de P3, dans
   cet ordre : concurrence d'abord (trois commits autonomes), puis déduplication, avec un
   snapshot du PKGBUILD posé **avant** de toucher `build_pkgbuild` — c'est ce qui rend le
   changement d'ordre des `sources` lisible dans le diff du commit qui le porte plutôt que
   découvert après coup. Deux bugs corrigés en chemin, tous deux trouvés en lisant le code
   avant d'écrire le plan : le décalage `format` de `description.xml`, et le message
   « sha1 mismatch » qui s'affichait avec deux sha1 identiques quand c'était un disque 2
   qui avait bougé. 160 → 181 tests.
8. **Ensuite** — reste de la dette P3 (`r`/`R`, packaging OpenBOR, et la migration
   reqwest 0.12 / rustls qui demande de bouger les trois dépôts ensemble), puis
   contribution SS sur base saine.
