## Plan : v0.18.0 — dette de dépendances, tests des fonctions pures, et UX en ligne de commande

**Type :** refactor + test + ui + conf
**Objectif :** solder P1.4 et P1.6 de `TODO.md`, puis livrer P2.1 → P2.6.
**Pourquoi :** le build n'est pas reproductible tant que 10 dépendances sont en `*` ;
les fonctions que P2.4 va modifier n'ont aucun test ; et rompom est aujourd'hui
inutilisable hors d'un terminal interactif alors que le README vend l'usage en CI.
**Étage(s) :** `conf`, `collect`, `pipeline`, `package`, `ui`, `nix`, `doc`

**Branche :** `feat/v0.18.0`
**Roadmap :** `TODO.md` P1.4, P1.6, P2.1, P2.2, P2.3, P2.4, P2.5, P2.6

### Périmètre

**In scope**
- P1.4 — pinning des dépendances, `[profile.release]`, suppression de `serde_derive`
- P1.6 — les six tests de fonctions pures qui manquent réellement (cf. `phase0_results.md` §1)
- P2.1 — CLI : `panic!` supprimé, `--version`, `--list-systems`, codes de sortie
- P2.2 — `--init` + validation de `lang`
- P2.3 — statut `retrying (2/3)…` visible
- P2.4 — trois cas limites multi-disc
- P2.5 — documentation (Nerd Fonts + `--ascii`, fichiers écrits dans le cwd, tier SS)
- P2.6 — `--plain` et `--resume=yes|no`
- Versionner le handoff design « turn 4 » (aujourd'hui dans `tmp/`, non commité)

**Out of scope**
- **P2.7 (refonte TUI « turn 4 »)** → v0.19. C'est une réécriture de `ui/render.rs`
  qui supprime `PANELS` / `RomPhase` / `render_active()` ; la mélanger à des correctifs
  CLI de quelques lignes rendrait la relecture et le changelog illisibles. Son
  prérequis (P1.2) est déjà satisfait, elle peut partir seule.
- **Migration reqwest 0.12 / rustls.** rompom ne peut pas bouger seul : `screenscraper`
  et `internetarchive` pinnent aussi reqwest 0.11. C'est un handoff vers les dépôts
  voisins, pas un lot rompom (P3).
- Toute la dette P3 restante, et la contribution SS (bloquée en amont).

### État du working tree

Propre, `master` @ `20621b4`, `just ci` vert (59 tests). Rien à faire disparaître.

---

## Lot A — P1.4 : la dette de dépendances

### Étape A1 : pinner les dix dépendances `*`

**Description :** remplacer chaque `"*"` par la version que `Cargo.lock` résout déjà
(cf. `phase0_results.md` §2), en `major.minor` là où Cargo le permet (`chrono = "0.4"`,
`reqwest = "0.11"`…) et en `major.minor.patch` pour ce qui a mordu (`reqwest = "0.11.27"`
est cité nommément dans `TODO.md`, et `serde_yaml 0.9.34+deprecated` est un cul-de-sac
qu'on ne veut pas voir bouger tout seul). `Cargo.lock` ne doit pas changer d'une ligne —
c'est la vérification.
**Vérification :** `just ci`, puis `git diff --stat Cargo.lock` doit être vide.
**Commit :** `build(deps): pin every dependency to the version already locked`

### Étape A2 : retirer `serde_derive`

**Description :** `serde` est déjà déclaré avec la feature `derive`, donc `serde_derive`
est une seconde porte vers les mêmes macros. Retirer la ligne de `Cargo.toml` et basculer
les deux `use serde_derive::…` (`src/conf/mod.rs:3`, `src/emulationstation.rs:2`) sur
`serde::…`.
**Vérification :** `just ci` — et surtout le snapshot XML de `generate_description_xml()`,
qui prouve que la sortie ne bouge pas d'un octet.
**Commit :** `chore(deps): drop serde_derive, redundant with serde's derive feature`

### Étape A3 : `[profile.release]` dans `Cargo.toml`

**Description :** déplacer les quatre réglages du bloc `env` de
`packages/rompom/default.nix` (`opt-level = "z"`, `lto = "thin"`, `codegen-units = 1`,
`strip = "symbols"`) dans un `[profile.release]` de `Cargo.toml`, et supprimer le bloc
`env` du Nix pour ne pas garder deux sources de vérité (les variables d'environnement
Cargo écrasent le manifeste : les laisser en double, c'est se garantir une divergence
silencieuse).

**Le commentaire de 12 lignes sur `panic = "abort"` déménage avec.** Il porte la raison
(sans lui `catch_unwind` est du code mort dans le binaire livré, cf. P0.3) *et* la mesure
(+483 Ko / +4,3 % le 2026-08-09). Sans ce commentaire dans `Cargo.toml`, le prochain qui
optimise la taille ajoute `panic = "abort"` et rouvre P0.3 sans le savoir.

**Vérification :** `just build-static` avant et après, comparer `ls -l result/bin/rompom` —
la taille doit être **identique**. Un binaire qui rétrécit soudainement de ~480 Ko veut dire
que `panic = "abort"` s'est glissé dedans.
**Commit :** `build: move the release profile from the Nix package into Cargo.toml`

### Étape A4 : remplacer `checksums` — **décision à valider**

**Description :** `checksums` tire `shaman`, qui tire `rustc-serialize 0.3.25`
(RUSTSEC-2022-0004, sans correctif amont) — deux des neuf avertissements acceptés dans
`.cargo/audit.toml`, et le **seul** que rompom puisse solder seul. rompom ne s'en sert que
pour SHA1/MD5/CRC32 ; les crates `sha1`, `md-5` et `crc32fast` (RustCrypto, maintenues)
couvrent exactement ça.

C'est le cœur de la correction : un sha1 faux fait retélécharger toute une bibliothèque, ou
pire, fait passer une ROM corrompue pour valide. Le commit porte donc ses tests de vecteurs
connus (chaîne vide, `"abc"`, un buffer > taille de bloc) **avant** la bascule, pour prouver
que l'ancienne et la nouvelle implémentation rendent la même chose.

**Si tu préfères ne pas y toucher dans ce lot**, on saute A4 : le reste de P1.4 tient
debout sans, et l'ignore reste justifié dans `.cargo/audit.toml`.
**Vérification :** `just ci` + `just audit` (l'ignore `RUSTSEC-2022-0004` doit pouvoir
sortir de `.cargo/audit.toml` sans rendre l'audit rouge) + un test manuel sur un vrai
système (`manual_tests.md` §1) pour vérifier qu'aucune ROM déjà en état ne se croit changée.
**Commit :** `refactor(pipeline): hash with the RustCrypto crates instead of checksums`
puis `chore(deps): stop ignoring RUSTSEC-2022-0004, the crate is gone`

---

## Lot B — P1.6 : les tests des fonctions pures

Ce lot passe **avant** P2.4 : il installe le filet avant qu'on touche au multi-disc.

### Étape B1 : extraire la collecte multi-disc vers `src/collect.rs`

**Description :** `disc_indicator()` et `group_multi_disc()` vivent dans `main.rs`, qui
fait 848 lignes et n'est pas un endroit où on va chercher de la logique. Déplacement pur,
sans changement de comportement — `TODO.md` P1.6 le demande explicitement, et le scope
`collect` de `PROCEDURE_PLANS.md` §3 existe déjà pour ça.
**Vérification :** `just ci`. Aucun test n'existe encore : le garde-fou de cette étape est
qu'elle ne change pas une ligne des deux fonctions (relecture du diff).
**Commit :** `refactor(collect): move multi-disc grouping out of main.rs`

### Étape B2 : couvrir `disc_indicator()` et `group_multi_disc()`

**Description :** figer le comportement **actuel**, y compris ce que P2.4 corrigera —
les trois bugs sont donc écrits comme tests dans le lot C, pas ici. Ici : indicateur
absent, `(Disc 1)` / `(Disk 2)` / `(CD 1)`, casse mixte, numérotation à partir de 0
(`Enemy Zero (USA) (Disc 0)`), tag région **avant** l'indicateur, groupement d'un jeu
3 disques, extension différente = pas de groupe, disque seul non groupé.
**Vérification :** `just ci`
**Commit :** `test(collect): cover disc detection and multi-disc grouping`

### Étape B3 : couvrir `search_name()` et `check_media_changes()`

**Description :** `search_name()` — extension retirée, tags région/révision retirés,
nom qui ne contient que des tags. `check_media_changes()` — média ajouté, média dont le
sha1 change, média retiré, état vide. Ces deux-là décident respectivement de ce qu'on
demande à ScreenScraper et de si le `pkgver` est bumpé ; se tromper coûte soit une requête
SS inutile, soit un paquet réécrit pour rien.
**Vérification :** `just ci`
**Commit :** `test(pipeline): cover search_name and media change detection`

### Étape B4 : couvrir `apply_game_path()` et `read_pkgver()`

**Description :** `apply_game_path()` — système 214 (`.sh`), 22 et 57 (`.m3u`),
multi-disc sur un système quelconque (`.m3u`), cas par défaut (chemin inchangé).
`read_pkgver()` — PKGBUILD absent → 0, `pkgver=3` → 3, PKGBUILD sans `pkgver` → 0,
`pkgver` non numérique → 0. Un `read_pkgver()` qui rend 0 à tort fait **régresser** le
`pkgver` d'un paquet déjà publié.
**Vérification :** `just ci`
**Commit :** `test(package): cover apply_game_path and read_pkgver`

### Étape B5 : réaligner `TODO.md` et `PROCEDURE_PLANS.md`

**Description :** les deux fichiers réclament encore le round-trip `SystemState` et
`apply_run_state()`, écrits en v0.16/v0.17 ; et `PROCEDURE_PLANS.md` §4 ouvre par
« rompom n'a **aucun** test aujourd'hui », faux depuis 59 tests.
**Vérification :** relecture
**Commit :** `docs: correct the test-debt statements left over from v0.16`

---

## Lot C — P2.4 : les trois cas limites multi-disc

Chaque étape est une **paire test rouge + correctif dans le même commit**
(`PROCEDURE_PLANS.md` §5). Le message de commit porte ce que le test produisait avant.

### Étape C1 : tag région perdu après l'indicateur

**Description :** `disc_indicator()` rend `stem[..paren]` comme base, donc tout ce qui
**suit** le groupe disque est jeté. `Game (Disc 1) (USA)` et `Game (Disc 2) (Europe)`
ont la même base `"Game"` et fusionnent en un seul paquet — deux régions différentes dans
un même `.m3u`. La base doit être le stem **privé du seul groupe disque**, ce qui suit
inclus.
**Vérification :** `just ci` ; le test échoue avant le correctif.
**Commit :** `fix(collect): keep what follows a disc indicator in the group name`

### Étape C2 : numéros de disque dupliqués

**Description :** deux fichiers réclamant le même numéro sont acceptés aujourd'hui ; l'un
écrase l'autre dans le `BTreeMap` ou les deux atterrissent dans le `.m3u`, dans un ordre
qui dépend de la collecte. C'est une bibliothèque mal nommée, pas une erreur de rompom :
le groupe est donc **refusé** (les fichiers repartent en entrées simples) et le
`--debug` en garde la trace, plutôt que de produire un `.m3u` faux en silence.
**Vérification :** `just ci` ; le test échoue avant le correctif.
**Commit :** `fix(collect): refuse to group two files claiming the same disc number`

### Étape C3 : `(CD32)` lu comme disque 32

**Description :** `lower.starts_with("cd")` sans séparateur avale `(CD32)`, le tag
plateforme Amiga CD32, et en tire « disque 32 ». Correctif : borner le numéro de disque à
une valeur plausible (au-delà, ce n'est pas un disque) **et** exiger un séparateur pour la
forme courte `cd`. `(CD 1)` et `(Disc 1)` restent reconnus.
**Vérification :** `just ci` ; le test échoue avant le correctif.
**Commit :** `fix(collect): stop reading the (CD32) platform tag as disc 32`

---

## Lot D — P2.1 : la ligne de commande

**Décision de procédure :** ces commits touchent l'analyse d'arguments et les codes de
sortie, un étage que la table de `PROCEDURE_PLANS.md` §3 ne nomme pas. J'ajoute une ligne
`cli` → `main.rs` (arguments, codes de sortie, usage) à la table, dans le commit D1.

### Étape D1 : plus de `panic!` sur argument invalide

**Description :** `main.rs:386` fait `Err(f) => panic!("{}", f)`. Une faute de frappe
(`--systm snes`) sort donc en trace de panique avec `RUST_BACKTRACE`, alors que le message
de `getopts` est parfaitement lisible. → message sur stderr, usage, `exit(2)`.
**Vérification :** `just ci` + `manual_tests.md` §2
**Commit :** `fix(cli): report an unknown argument instead of panicking`

### Étape D2 : codes de sortie cohérents

**Description :** système inconnu (`main.rs:449`) et système sans `source`
(`main.rs:457`) impriment sur stderr puis `return` — donc **exit 0**. En CI, un système
mal orthographié passe pour un succès. → `exit(1)`. Table des codes : 0 succès,
1 erreur d'exécution, 2 erreur d'usage. Le cas « aucune ROM à traiter » reste 0.
Documentée dans le README.
**Vérification :** `just ci` + `manual_tests.md` §2
**Commit :** `fix(cli): exit non-zero when the system is unknown or has no source`

### Étape D3 : `--version`

**Description :** `env!("CARGO_PKG_VERSION")`. Trivial, mais c'est la première chose
qu'on demande à un binaire quand on rapporte un bug, et `just version-check` garantit
déjà que cette version est la vraie.
**Vérification :** `just ci`
**Commit :** `feat(cli): add --version`

### Étape D4 : `--list-systems`

**Description :** lister les systèmes du `rompom.yml` chargé, avec leur id et le type de
source (`internet_archive` / `folder` / **aucune**). C'est la réponse à « pourquoi mon
système est inconnu » que D2 vient de rendre bruyante.
**Vérification :** `just ci` + `manual_tests.md` §2
**Commit :** `feat(cli): add --list-systems`

---

## Lot E — P2.2 : `--init` et validation de `lang`

### Étape E1 : `rompom --init`

**Description :** écrire un `~/.config/rompom.yml` de départ à partir du `rompom.yml` de
la racine, embarqué par `include_str!` (donc jamais désynchronisé du dépôt). **Refuse
d'écraser** un fichier existant : le fichier contient les credentials de l'utilisateur, un
`--init` distrait ne doit pas les effacer. Sortie 1 si le fichier est déjà là, en disant
lequel.
**Vérification :** `just ci` + `manual_tests.md` §3
**Commit :** `feat(cli): write a starter configuration with --init`

### Étape E2 : valider `lang` au chargement

**Description :** `Conf::load()` accepte aujourd'hui n'importe quel code de langue. Un
`lang: [fr-FR]` ne fait pas échouer le chargement, il fait juste que ScreenScraper ne
rend jamais de synopsis — et l'utilisateur croit que les descriptions sont manquantes en
amont. Valider contre `SUPPORTED_LANGS` (déjà là, `conf/mod.rs:9`) avec une variante
d'erreur qui liste les codes acceptés.
**Vérification :** `just ci` (test unitaire sur `Conf::load`)
**Commit :** `fix(conf): reject a language code ScreenScraper does not serve`

---

## Lot F — P2.3 et l'option `--ascii`

### Étape F1 : afficher les retries

**Description :** `execute_step` retente aujourd'hui en silence, avec un backoff de 1, 2
puis 4 secondes. Vue de la TUI, la ROM est simplement figée. → `RomBar::retrying(n, max)`
→ statut `retrying (2/3)…` en jaune. C'est aussi la moitié du diagnostic quand un run
traîne : on voit *qui* retente.
**Vérification :** `just ci` + `manual_tests.md` §4
**Commit :** `feat(ui): show the retry attempt on the bar`

### Étape F2 : `--ascii`

**Description :** les icônes de `MEDIA_ICONS` sont des glyphes Nerd Font ; sans la police,
la TUI est une rangée de tofu. `--ascii` bascule sur une table d'équivalents ASCII, au même
endroit (`MEDIA_ICONS` est déjà centralisé, `ui/mod.rs`). P2.7 reprendra ces glyphes tels
quels — la bascule doit rester une donnée, pas du code de rendu.
**Vérification :** `just ci` + `manual_tests.md` §4
**Commit :** `feat(ui): add --ascii for terminals without a Nerd Font`

---

## Lot G — P2.6 : le mode non interactif

Trois bloqueurs, un par étape. C'est le gros morceau du lot et il conditionne le cas
d'usage CI annoncé dans le README (§ *Automation*).

### Étape G1 : `--resume=yes|no`

**Description :** premier bloqueur. `main.rs:476-483` fait `read_line` sur stdin ; en CI,
stdin est fermé, `read_line` rend `Ok(0)`, la réponse est `""` → « yes » par accident.
`--resume=yes|no` court-circuite le prompt. Sans le drapeau et sans tty, la valeur par
défaut est **`no`** (fichier `run.yml` supprimé, run neuf) : reprendre à l'aveugle un run
dont on ne sait rien est le comportement le plus surprenant des deux.
**Vérification :** `just ci` + `manual_tests.md` §5
**Commit :** `feat(cli): answer the resume prompt up front with --resume`

### Étape G2 : `--plain`

**Description :** deuxième bloqueur. `Ui::new()` fait `enable_raw_mode().unwrap()` +
`EnterAlternateScreen` sans condition (`ui/mod.rs:365-369`) : hors tty ça échoue ou ça
crache des séquences d'échappement dans le log de CI. `--plain` prend un chemin de rendu
ligne par ligne (une ligne par ROM terminée, pas de thread de rendu, pas de raw mode), et
**l'absence de tty l'active implicitement**. `Summary::print()` ne bouge pas : il n'a
jamais dépendu de ratatui, c'est déjà le bilan du mode plain.

Conséquence à ne pas rater : c'est le thread de rendu qui capte le Ctrl-C depuis que raw
mode mange `ISIG` (cf. `CLAUDE.md`). Sans lui, on revient au `SIGINT` classique — le
handler `ctrlc` est déjà en place et le couvre, mais il faut le vérifier réellement
(`manual_tests.md` §5).
**Vérification :** `just ci` + `manual_tests.md` §5
**Commit :** `feat(ui): add a non-interactive --plain mode`

### Étape G3 : la modale en mode plain

**Description :** troisième bloqueur. Une ROM que ScreenScraper ne connaît pas ouvre une
modale bloquante ; sans terminal, il n'y a personne pour répondre. Le step échoue alors
avec une cause explicite (« not identified — needs manual identification »), la ROM
apparaît dans la section `Failures`, et **le run continue**. Ni blocage, ni identification
au hasard : emballer un paquet sur un jeu deviné produirait un mauvais `description.xml`
avec un `pkgver` bumpé, exactement le dégât que P1.1 vient de fermer.
**Vérification :** `just ci` + `manual_tests.md` §5
**Commit :** `feat(pipeline): fail unidentified ROMs instead of blocking without a terminal`

---

## Lot H — P2.5 : documentation

### Étape H1 : ce que la doc ne dit pas

**Description :** quatre trous listés par P2.5, tous dans README + `CLAUDE.md` :
police Nerd Font requise (et `--ascii` comme repli, cf. F2) ; les fichiers écrits **dans
le répertoire courant** (`<system>.state.yml`, `<system>.run.yml`, `<system>.debug.log`) ;
ce que coûte la suppression de `state.yml` (tout est retéléchargé et chaque `pkgver` est
bumpé) ; et le lien entre le tier du compte ScreenScraper et `maxthreads`, donc la vitesse
du run. Le reste (drapeaux CLI, codes de sortie) est déjà arrivé avec ses commits.
**Vérification :** relecture
**Commit :** `docs: document the Nerd Font requirement and the files rompom writes`

---

## Lot I — clôture

### Étape I1 : versionner le handoff design « turn 4 »

**Description :** `tmp/design_handoff_rompom_tui/` (README.md + mockups.dc.html) est la
spec complète de P2.7 et `tmp/` est du scratch non versionné — un `rm -rf tmp` et la spec
est perdue. → `design/tui/`.
**Vérification :** relecture
**Commit :** `docs(ui): commit the turn-4 TUI design handoff`

### Étape I2 : mettre à jour la roadmap

**Description :** cocher P1.4, P1.6, P2.1 → P2.6 dans `TODO.md`, avec pour chacun ce qui a
été trouvé en chemin (la valeur de ce fichier est là, pas dans les cases cochées). Mettre
à jour le séquencement : v0.19 = P2.7 seul.
**Vérification :** relecture
**Commit :** `docs: record what v0.18.0 closed in the roadmap`

### Étape I3 : release

**Description :** `just release 0.18.0`, relire le diff du changelog.
**Vérification :** `just ci` + `just version-check` + `just changelog-preview`
**Commit :** `chore(release): v0.18.0`

---

## Décisions techniques

1. **P2.7 reste dehors.** Prérequis satisfait (P1.2), mais c'est une réécriture de
   `ui/render.rs` : la mélanger à des correctifs CLI de quelques lignes rend le lot
   irrelisable. Et F1/F2 lui préparent le terrain sans la contraindre (le statut retry
   et la table d'icônes sont des données, pas du rendu).

2. **Les tests (lot B) avant les correctifs (lot C).** P2.4 modifie `disc_indicator()`
   et `group_multi_disc()`, aujourd'hui sans aucun test. B2 fige le comportement actuel,
   C1-C3 changent ce qui doit changer — et le diff de tests montre exactement quoi.

3. **`[profile.release]` dans `Cargo.toml`, plus dans le Nix.** Les variables
   d'environnement Cargo écrasent le manifeste ; garder les deux, c'est se garantir une
   divergence qu'aucune porte ne détecte. Le commentaire sur `panic = "abort"` déménage
   avec le profil, sinon il protège un fichier qui ne décide plus rien.

4. **Duplicat de numéro de disque → refus de grouper** (C2), pas « garder le premier ».
   Rompom ne peut pas savoir lequel est le bon ; deux paquets simples sont réparables à la
   main, un `.m3u` faux ne se voit qu'à la manette.

5. **Hors tty : `--resume=no` par défaut** (G1). Reprendre un `run.yml` dont personne n'a
   validé la provenance est le plus surprenant des deux comportements, et le coût du
   contraire est faible — tout ce qui est cher est déjà idempotent (`CLAUDE.md`,
   « Interrupted run / resume »).

6. **Hors tty : ROM non identifiée → échec explicite** (G3), jamais une identification
   devinée. Emballer sur un mauvais jeu écrit un `description.xml` faux avec un `pkgver`
   bumpé — le dégât exact que P1.1 a fermé.

7. **A4 (sortir `checksums`) est une décision à valider**, pas un acquis. C'est la seule
   dette d'audit que rompom puisse solder seul, mais elle touche le calcul de sha1, donc
   le cœur de « ce fichier a-t-il changé ». Si le lot doit rester petit, on saute A4.

## Portes de qualité

- [ ] `just ci` passe à **chaque** commit, pas seulement à la fin
- [ ] `just audit` vert (et un ignore de moins dans `.cargo/audit.toml` si A4 est retenue)
- [ ] `nix flake check` vert
- [ ] `just build-static` : taille du binaire inchangée après A3
- [ ] Tests ajoutés pour tout code pur touché ; C1-C3 observés **rouges** avant correctif
- [ ] Doc synchronisée dans le même commit que le code
- [ ] Commits atomiques, scope réel (jamais `all`), sujets de qualité changelog
- [ ] Branche `feat/v0.18.0`, **non mergée par Claude**
- [ ] `manual_tests.md` exécuté et ses résultats consignés avant la release
