## Plan : v0.20.0 — la progression des téléchargements, et un lot de dette P3

**Type :** ui + pipeline + package + refactor
**Objectif :** fermer une fuite de credentials sur le chemin médias, puis brancher ce que
`internetarchive` v0.3.0 et `screenscraper` v0.8.0 viennent de rendre possible —
l'avancement d'un téléchargement — et solder trois points de P3.
**Pourquoi :** la colonne `rom` de la grille montre un spinner là où la spec demande
`62 %`, et le débit affiché n'avance que quand un fichier se termine. Les deux libs
exposent maintenant ce qu'il faut. À côté, deux bugs de templates sont dans le dépôt depuis
le début et deux structures traînent des champs que plus rien ne lit.
**Étage(s) :** `pipeline`, `ui`, `package`, `nix`, `doc`

**Branche :** `feat/v0.20.0`
**Roadmap :** `TODO.md` P3 — progression par téléchargement, bugs de templates, code mort,
`m.url`
**Amont :** `.claude/plans/v0.19.0/handoff_*.md`, livrés en `internetarchive` v0.3.0 et
`screenscraper` v0.8.0

---

### Périmètre

**In scope**

- Bump des deux libs (Cargo.toml, Cargo.lock, `cargoLock.outputHashes` du paquet Nix)
- `Cell::Progress(u8)` dans la colonne `rom`, alimentée par le callback
- Débit et volume à l'octet plutôt qu'au fichier terminé, **compteur qui recule compris**
- Ligne de détail `rom 2.4/3.9 MiB` sur le ROM sélectionné
- `mkdir -p 0700 -p` → `mkdir -m 0700 -p` (4 templates)
- `ls *.mp4 *.png *.xml *.pdf, *.jpg` → virgule parasite retirée (4 templates)
- Code mort : `Phase` + `StepKind::phase()`, les charges utiles de `StepData` que rien ne
  lit, `Package.name`
- **Fuite de credentials sur le chemin médias** : rompom cite le `Display` de l'erreur de
  la lib, qui contient `media.url` — donc `devpassword` et `sspassword`
  (`phase0_results.md` §4.2)
- `media_region()` renommée et durcie (§4.3)

**Out of scope — et pourquoi**

- **Déduplications P3** (`rom_unchanged` ×2, les 8 médias ×4, `copy_rom`/`download_rom`,
  les 8 blocs de `build_pkgbuild`) : c'est du refactor à snapshot constant, sûr mais
  volumineux, et le mélanger à un branchement de callback rendrait le diff illisible.
  Prochain lot.
- **`m.url` au lieu des URLs reconstruites à la main** : **annulé, et l'item P3 est
  réécrit en avertissement.** `m.url` est l'URL API, credentials compris ; la
  reconstruction est un blanchiment délibéré, pas une duplication naïve
  (`phase0_results.md` §4.1).
- **`.without_url()` côté screenscraper** : dépôt voisin, et point distinct du handoff.
  Reste P3.
- **`r`/`R`**, **packaging OpenBOR**, **migration reqwest 0.12 / rustls** : chacun son
  plan.

---

### État du working tree

Propre, `master` @ `904c8f8`, `v0.19.0` taguée, `just ci` vert (146 tests).

Ce qui **doit** avoir disparu à la fin : `Phase`, `StepKind::phase()`, `Package.name`, et
les variantes de `StepData` dont plus rien ne lit la charge utile.

---

### Fichiers touchés

- [ ] `Cargo.toml`, `Cargo.lock`, `packages/rompom/default.nix` — bump des deux libs
- [ ] `src/ui/mod.rs` — `Cell::Progress`, `RomBar::rom_progress()`, cumul d'octets
- [ ] `src/ui/render.rs` — glyphe de la cellule, ligne de détail
- [ ] `src/ui/rate.rs` — delta d'un compteur qui peut reculer
- [ ] `src/ui/errors.rs` — `TransferFailed` rangée explicitement
- [ ] `src/worker/handlers/downloads.rs` — les trois branchements du callback
- [ ] `src/rom/step.rs`, `src/rom/mod.rs`, `src/worker/mod.rs` — code mort
- [ ] `src/package.rs` — `Package.name`, `media_slug()`
- [ ] `src/worker/helpers.rs` — `media_failure()`
- [ ] `assets/templates/pkgbuild/{ps2,psx,multidisc,segacd}-package.jinja` — les deux bugs
- [ ] `CLAUDE.md` / `README.md` / `TODO.md`

---

### Décisions techniques

**D1 — Le compteur qui recule est traité à la source, pas au bord.**
`internetarchive` tronque le fichier et repart de zéro quand il bascule de miroir
(`phase0_results.md` §3.1). Plutôt que de laisser chaque appelant se débrouiller,
`RomBar::rom_progress(read, total)` calcule le delta contre ce qu'il a vu la dernière fois
**pour ce ROM**, et traite tout recul comme un redémarrage : le delta est nul, pas négatif.
`Rate` continue de recevoir un cumul monotone, donc sa soustraction reste correcte.

**D2 — `Cell::Progress(u8)` n'apparaît que sur le chemin qui la produit.**
`CopyRom` reste au spinner : `fs::copy` ne rend pas la main, et une copie locale n'est
presque jamais l'attente qui compte. Seuls `DownloadRom` et `DownloadMedias` la posent.

**D3 — Le volume passe à l'octet, mais reste compté une seule fois.**
Aujourd'hui `bar.rom_done(bytes)` ajoute la taille du fichier terminé. Avec le callback,
les octets arrivent en continu ; `rom_done` ne doit donc plus rien ajouter, sinon chaque
fichier est compté deux fois. Un ROM sauté (`rom_unchanged`, sha1 déjà bon) n'ajoute rien
du tout, ce qui est correct : rien n'a transité.

**D4 — Le callback est appelé sous le verrou du `Rom`, jamais pendant l'I/O.**
`RomBar` prend le verrou de `AppState`, pas celui du `Rom` : le handler tient déjà le
second pendant qu'il télécharge. Le callback ne doit rien faire d'autre que poser deux
nombres — tous les 64 Kio, sur des fichiers de 700 Mo, c'est ~11 000 prises de verrou par
ROM. Acceptable parce que le verrou n'est jamais tenu plus de quelques instructions, mais
c'est la raison pour laquelle il n'y a pas de `format!` dedans.

**D5 — rompom ne cite plus jamais l'erreur de la lib média.**
Exactement le remède de P1.1, appliqué au chemin qu'elle n'avait pas audité : `media_failure()`
compose sa phrase depuis la **variante** de l'erreur, jamais depuis son `Display`. Les deux
variantes réseau (`Download`, `Body`) portent `media.url` et sont donc réduites à une
phrase constante ; `Io` et `ChecksumMismatch` ne portent qu'un chemin local et deux sha1,
qui sont utiles et sûrs.

Ces deux-là sont aussi les seules constructibles dans un test — `reqwest::Error` n'a pas de
constructeur public. Le test couvre donc ce qu'il peut construire, et la garantie sur les
deux autres est **structurelle** : la fonction n'interpole jamais `err`.

**D6 — `StepData` est allégé, pas supprimé.**
`LookupSS { candidates }` est bel et bien lu (`discovery.rs:365`, pour remplir la modale).
Ce qui part, c'est ce que plus rien ne relit : `WaitModal { jeu }` (écrit « pour la
télémétrie », jamais consulté), et les charges de `ComputeHashes` et `BuildPackage`, dont
les vraies valeurs vivent dans le `Rom`. `TODO.md` disait « `StepData` quasi entier » —
« quasi » était le mot juste.

---

### Étapes atomiques

#### Étape 1 : la fuite de credentials sur le chemin médias
**Description :** `media_failure(kind, err)` dans `worker/helpers.rs` remplace le
`format!("media {}: {}", kind, e)` de `handle_download_medias`. Test de régression écrit
**avant** le correctif et observé rouge, commité **avec** lui (`PROCEDURE_PLANS.md` §5).
**Tests :** aucune des phrases produites ne contient `devpassword`, `sspassword`, `devid`,
`ssid` ni `http` ; le sha1 attendu et obtenu restent visibles sur `ChecksumMismatch` ; le
chemin local reste visible sur `Io`.
**Vérification :** `just ci`
**Commit :** `fix(pipeline): stop printing the ScreenScraper media URL, credentials and all`

#### Étape 2 : `media_region()` renommée et durcie
**Description :** `media_slug()` — elle rend le slug média, pas une région. `find("media=")`
prenait tout ce qui suit, donc un paramètre ajouté après par SS finirait dans le nom de
fichier ; borné au prochain `&`.
**Tests :** les slugs des exemples réels (`sstitlejp`, `box-2Djp`, `ss(jp)`, `manueljp`),
un `media=` suivi d'un autre paramètre, un `media=` absent.
**Vérification :** `just ci`, snapshot PKGBUILD inchangé
**Commit :** `fix(package): read the media slug without swallowing what follows it`

#### Étape 3 : bump des deux libs
**Description :** `tag = "v0.3.0"` (internetarchive) et `tag = "v0.8.0"` (screenscraper)
dans `Cargo.toml`, `Cargo.lock` régénéré, `cargoLock.outputHashes` du paquet Nix mis à
jour (lancer `nix build`, recopier les hashes `got:`). Aucun appel ne change : les deux
libs ont gardé leur `fetch`.
**Vérification :** `just ci` **et** `nix build` — c'est le seul qui vérifie les hashes.
**Commit :** `chore(deps): bump internetarchive to v0.3.0 and screenscraper to v0.8.0`

#### Étape 4 : l'avancement d'un téléchargement
**Description :** `Cell::Progress(u8)` + `RomBar::rom_progress(read, total)` (delta
monotone, cf. D1) ; `handle_download_rom` et `handle_download_medias` passent à
`fetch_with_progress`. `rom_done()` ne compte plus d'octets (D3). Le glyphe `62%` dans une
colonne de 6 cellules, comme la spec l'a calibrée.
**Tests :** formatage du pourcentage (0, 62, 100, `total` inconnu), et le delta d'un
compteur qui recule.
**Vérification :** `just ci`
**Commit :** `feat(ui): show how far a download has got`

#### Étape 5 : débit et volume à l'octet
**Description :** `Rate` alimentée en continu. Le garde-fou anti-recul vit dans
`RomBar::rom_progress`, mais `Rate::tick` gagne aussi une soustraction saturante — deux
défenses, parce qu'un cumul non monotone est une panique en debug et un compteur bouché en
release. `errors::classify()` range explicitement `Transfer interrupted` dans `download`
au lieu de compter sur son `https`.
**Tests :** `Rate::tick` sur un cumul qui recule ; `classify("Transfer interrupted on …")`.
**Vérification :** `just ci`
**Commit :** `feat(ui): count transfer volume as it happens`

#### Étape 6 : la ligne de détail du transfert
**Description :** sur le ROM sélectionné, ligne 3 devient `rom 2.4/3.9 MiB` quand un
transfert est en cours, à la place du répertoire de sortie.
**Tests :** la mise en forme (`grid::format_bytes` est déjà couverte ; ce qui s'ajoute est
le choix de ligne).
**Vérification :** `just ci`
**Commit :** `feat(ui): show the transfer in the selected ROM's details`

#### Étape 7 : `mkdir -m 0700`
**Description :** `mkdir -p 0700 -p …` crée un répertoire **nommé `0700`** puis les
autres, et les droits voulus ne sont jamais posés. Quatre templates.
**Tests :** snapshot du `package()` rendu.
**Vérification :** `just ci`
**Commit :** `fix(package): create the ROM data directory with the mode it asks for`

#### Étape 8 : la virgule parasite du glob
**Description :** `ls *.mp4 *.png *.xml *.pdf, *.jpg` cherche un fichier littéralement
nommé `*.pdf,` — `ls` râle et le manuel n'est jamais installé. Quatre templates.
**Tests :** snapshot.
**Vérification :** `just ci`
**Commit :** `fix(package): stop looking for a file named "*.pdf,"`

#### Étape 9 : le code mort du pipeline
**Description :** `Phase` et `StepKind::phase()` supprimés (ils routaient vers les panneaux
disparus en v0.19) ; charges utiles de `StepData` que rien ne relit (D6).
**Vérification :** `just ci`
**Commit :** `refactor(pipeline): drop the step payloads nothing reads`

#### Étape 10 : `Package.name`
**Description :** `name` et `rom` reçoivent la même valeur au constructeur
(`package.rs:257` et `:262`). `name` part, ses lecteurs passent sur `rom`.
**Vérification :** `just ci`, snapshot inchangé — sinon ce n'était pas un refactor.
**Commit :** `refactor(package): drop Package.name, a second copy of Package.rom`

#### Étape 11 : the P3 entry that must not be done
**Description :** `TODO.md` — l'item « utiliser `m.url` » devient un **avertissement**
expliquant pourquoi le reconstruire à la main est délibéré. Sans ça, quelqu'un le
« corrigera » un jour.
**Commit :** `docs: warn that m.url must not reach a PKGBUILD`

#### Étape 12 : roadmap
**Commit :** `docs: record what v0.20.0 closed in the roadmap`

#### Étape 13 : release
**Description :** `just release 0.20.0`, relecture du changelog généré.
**Vérification :** `just ci`, `nix flake check`, `just audit`, `nix build`
**Commit :** `chore(release): v0.20.0`

---

### Portes de qualité

- [ ] `just ci` passe à chaque étape
- [ ] `nix build` à l'étape 1 (les hashes) **et** avant la release
- [ ] `nix flake check` et `just audit` avant la release
- [ ] Tests ajoutés pour tout le code pur introduit
- [ ] Doc synchronisée dans le même commit que le code
- [ ] Commits atomiques, scope réel, jamais `all`
- [ ] Branche `feat/v0.20.0`, non mergée par Claude
- [ ] `manual_tests.md` exécuté par le mainteneur — le pourcentage et le débit ne se
      vérifient qu'à l'œil, sur un vrai téléchargement
