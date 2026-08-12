## Plan : v0.21.0 — dédupliquer les médias et les disques, dégripper la concurrence

**Type :** refactor + perf + bug
**Objectif :** solder les deux premiers points ouverts de P3 — les quatre duplications
listées (`rom_unchanged`, la liste des 8 médias, `copy_rom`/`download_rom`, les 8 blocs de
`build_pkgbuild`) et les trois points de concurrence (`debug_assert!` anti-wrap,
`cancelled` sous le mutex du sémaphore, retry sans `thread::sleep` bloquant).
**Pourquoi :** ces duplications ne sont pas cosmétiques. La liste des 8 médias est écrite
**cinq** fois avec **deux ordres différents**, et une sixième table (`MEDIA_ICONS`) fixe
l'ordre à l'écran : ajouter un asset demande aujourd'hui six éditions cohérentes, et une
seule oubliée décale les pastilles ou perd un fichier. Côté concurrence, le
`thread::sleep` du backoff immobilise un worker jusqu'à 16 s — et retarde d'autant le
Ctrl-C, qui ne peut pas le réveiller.
**Étage(s) :** `pipeline`, `package`, `doc`

**Branche :** `refactor/v0.21.0`
**Roadmap :** `TODO.md` P3 — points 1 (dédup) et 3 (concurrence)

---

### Périmètre

**In scope**

- `debug_assert!` anti-underflow dans `Step::dec_wait_for()`
- `Semaphore.cancelled` déplacé sous le mutex → suppression du réveil toutes les 50 ms
- Backoff de retry porté par la `TaskQueue` (`push_after`) au lieu d'un `thread::sleep`
  dans le worker
- Une seule table des 9 assets, dérivée de `MEDIA_ICONS`, consommée par les **six** sites
- `rom_unchanged` calculé par une fonction pure unique, testée
- `CopyRom` / `DownloadRom` : boucle sur les disques partagée
- `build_pkgbuild` : les 8 blocs `if let Some(ref x) = self.medias.…` deviennent une boucle
- Correction du décalage `x.format` brut vs `media_ext()` dans `description.xml`
- Snapshot PKGBUILD (filet avant de toucher `build_pkgbuild`)
- MAJ `CLAUDE.md` et `TODO.md`

**Out of scope**

- `r` / `R` (relance des ROMs échouées) — son propre plan, cf. `TODO.md`
- Migration rustls / reqwest 0.12 — demande de bouger les trois dépôts ensemble
- Packaging OpenBOR — à instruire avec un vrai jeu avant de décider quoi corriger
- Toute modification de `Medias` en tableau indexé : la struct à champs nommés reste, seuls
  ses **parcours** sont unifiés

---

### État du working tree

`master` à `29b3e55`, propre. Phase 0 verte, cf. `phase0_results.md` (160 tests).

---

### Ce que le code fait aujourd'hui (constaté, pas supposé)

**Les six écritures de la liste des médias**, et leurs ordres :

| site | fichier | ordre |
|---|---|---|
| pastilles de la grille | `ui/mod.rs:47` `MEDIA_ICONS_NERD` | description, video, image, thumbnail, **screenshot, bezel**, marquee, wheel, manual |
| pastilles du modal | `worker/helpers.rs:170` `CANDIDATE_MEDIA` | idem (noms SS) |
| construction | `package.rs:274` `Package::new` | noms SS, en dur |
| sources PKGBUILD | `package.rs:379-440` | video, **bezel**, image, thumbnail, marquee, **screenshot**, wheel, manual |
| téléchargement | `handlers/downloads.rs:256` | video, image, thumbnail, **bezel**, marquee, **screenshot**, wheel, manual |
| état + diff | `helpers.rs:100`, `save_state.rs:21` | idem téléchargement |

Deux ordres, donc, et la bascule `video-normalized` → `video` est écrite deux fois
(`package.rs:279` et `helpers.rs:196`).

**`rom_unchanged`** : `discovery.rs:121-159` (ComputeHashes) et `discovery.rs:201-237`
(LookupSS) sont le même calcul et la même cascade de messages de debug, au préfixe près.

**`build_pkgbuild`** : huit blocs identiques à trois exceptions près — `video` s'appelle
toujours `video.mp4`, `manual` toujours `manual.pdf`, les six autres `{kind}.{ext}`.
C'est **exactement** la règle de `media_filename()` (`helpers.rs:80`), réécrite à la main.

**Le décalage `format`** : `make_game()` (`package.rs:518-538`) écrit
`./data/{romname}/thumbnail.{x.format}` avec le format **brut** de ScreenScraper, alors
que le fichier réellement posé sur le disque s'appelle `media_filename(kind, format)`,
c'est-à-dire `thumbnail.{media_ext(format)}`. `media_ext()` a été introduit en P0.2
précisément parce que ce champ n'est pas fiable : sur un format hors liste blanche il rend
`bin`, et `description.xml` pointe alors sur un fichier qui n'existe pas.

**Le sleep du backoff** : `worker/mod.rs:253`. Le worker dort 1, 2, 4, 8 ou 16 s, hors
queue. Un Ctrl-C ne le réveille pas — `queue.shutdown()` et `ss_sem.cancel()` ne touchent
ni l'un ni l'autre un thread endormi.

**`dec_wait_for`** (`rom/step.rs:170`) : `fetch_sub(1, SeqCst) - 1`. À zéro, `fetch_sub`
enroule sur `usize::MAX` et l'expression rend `MAX - 1` : le test `remaining == 0` est
faux, le successeur n'est **jamais** poussé, la ROM ne finit pas et le run ne s'arrête
plus. Un bug de DAG se paierait donc en blocage silencieux.

---

### Décisions techniques

**D1 — Une seule table, et c'est celle de l'écran.** `MEDIA_ICONS` fixe déjà l'ordre des
colonnes ; la nouvelle table `MEDIA_KINDS` (dans `package.rs`, à côté de `Medias`) porte
`(kind, ss_name)` dans **cet** ordre, et `ui::MEDIA_ICONS` reste la source de l'icône.
Conséquence assumée : l'ordre de téléchargement suit désormais l'ordre des pastilles, qui
se remplissent donc de gauche à droite.

**D2 — L'ordre des `sources` du PKGBUILD change.** C'est la conséquence directe de D1, et
c'est le seul écart visible du lot. Sans risque : `makepkg` ne lit `sources`/`sha1sums`
que par correspondance de position, que la boucle maintient par construction, et les
templates désignent les fichiers par leur nom. Le PKGBUILD n'entre pas dans la décision de
bump (`package_changed` regarde le ROM, les médias et `description.xml`), donc aucun
paquet n'est réécrit du seul fait de ce commit. Le snapshot ajouté en B1 rend le
changement lisible dans le diff du commit qui le porte — c'est un acte intentionnel, pas
un effet de bord découvert plus tard.

**D3 — `media_filename()` remonte dans `package.rs`.** Elle nomme un fichier de média ;
`media_ext()`, `media_url()` et `Medias` sont déjà là. `downloads.rs` l'importe depuis
`package` comme il importe déjà `media_url`. Cela permet à `build_pkgbuild` de l'appeler
au lieu de réécrire sa règle.

**D4 — Le décalage `format` est un bug, pas un refactor.** Il part dans son **propre**
commit `fix(package)`, avec son test de régression écrit d'abord et vu rouge, comme
l'impose `PROCEDURE_PLANS.md` §5. Sur un format normal (`png`, `jpg`) le rendu est
identique — le snapshot `description_xml_snapshot` ne bouge pas.

**D5 — Le retry passe par la queue, pas par un thread endormi.** `TaskQueue` reçoit
`push_after(rom, idx, delay)` et un tas de tâches datées ; `pop_main()` attend jusqu'à la
plus proche échéance au lieu d'attendre indéfiniment. Deux gains, un choix :
- le worker retourne immédiatement dans le pool au lieu d'être immobilisé jusqu'à 16 s ;
- un Ctrl-C pendant un backoff est pris en compte tout de suite.
- **Choix :** `shutdown()` **abandonne** les tâches datées. Elles sont `Pending`, donc
  `collect_run_state()` les écrit dans `run.yml` et le resume les rejoue — les
  ressusciter à l'arrêt ne ferait que retarder la sortie.

Seule la voie `main` porte des tâches datées : `WaitModal`, unique step de la voie
bloquante, ne rend jamais `Transient` (elle rend `Done`, `Fatal` ou `Interrupted`).

**D6 — `cancelled` sous le mutex.** Le drapeau devient un champ de l'état protégé, et
`acquire()` repasse sur un `cvar.wait()` sans timeout. Aujourd'hui chaque attente se
réveille 20 fois par seconde pour relire un `AtomicBool` ; avec `maxthreads` petit et des
centaines de ROMs, ce sont des réveils permanents pour rien. `cancel()` pose le drapeau
sous le verrou puis `notify_all()`, donc aucun réveil ne peut être manqué.

---

### Fichiers touchés

- [ ] `src/queue.rs` — `Semaphore.cancelled`, `push_after`, tas daté
- [ ] `src/rom/step.rs` — `debug_assert!` dans `dec_wait_for`
- [ ] `src/worker/mod.rs` — `Disposition::Retry` → `push_after`
- [ ] `src/package.rs` — `MEDIA_KINDS`, `Medias::iter()`, `media_filename()`,
      `Package::new`, `build_pkgbuild`, `make_game`
- [ ] `src/worker/helpers.rs` — `check_media_changes`, `candidate_from`, départ de
      `media_filename`
- [ ] `src/worker/handlers/discovery.rs` — `rom_unchanged` unique
- [ ] `src/worker/handlers/downloads.rs` — boucle disques partagée, boucle médias
- [ ] `src/worker/handlers/save_state.rs` — `medias_to_sha1_map`
- [ ] `CLAUDE.md`, `TODO.md`

---

### Étapes atomiques

#### Phase A — Concurrence

**A1 — `debug_assert!` anti-underflow**
Description : `dec_wait_for()` assène que la valeur précédente était non nulle. En release
le code est inchangé ; en test et en debug, un DAG mal câblé panique au lieu de bloquer le
run pour toujours. Test : un `Step` à `wait_for = 0` panique en debug.
Vérification : `just ci`
Commit : `fix(pipeline): catch a wait_for underflow before it hangs the run`

**A2 — Sémaphore sans polling**
Description : `cancelled` passe dans l'état sous mutex, `acquire()` attend sans timeout.
Test : un waiter bloqué sans permit est débloqué par `cancel()` et rend `false` (thread +
`recv_timeout`, pas de terminal ni de réseau).
Vérification : `just ci`
Commit : `perf(pipeline): let the semaphore sleep until it is released or cancelled`

**A3 — Retry porté par la queue**
Description : `TaskQueue::push_after()` + tas daté ; `pop_main()` attend jusqu'à la
prochaine échéance ; `execute_step` remplace `thread::sleep(delay)` par `push_after`.
Tests : une tâche datée n'est pas rendue avant son échéance et l'est après ; `shutdown()`
avec une tâche datée en attente rend `None` sans attendre l'échéance.
Vérification : `just ci`
Commit : `perf(pipeline): schedule retries in the queue instead of parking a worker`

#### Phase B — Déduplication

**B1 — Filet : snapshot du PKGBUILD**
Description : un test qui construit un `Package` avec les huit médias renseignés et
compare le PKGBUILD rendu à un attendu littéral. Vert sur le code actuel — c'est ce qui
rend le diff de B2 lisible.
Vérification : `just ci`
Commit : `test(package): snapshot the generated PKGBUILD`

**B2 — Une table pour les neuf assets**
Description : `MEDIA_KINDS` + `Medias::iter()` dans `package.rs`, `media_filename()`
rapatriée (D3), `Package::new` et `build_pkgbuild` pilotés par la table. Le snapshot de B1
bouge : l'ordre des sources suit désormais celui de l'écran (D2), et le diff ne montre que
cela. Doc : `CLAUDE.md` (§ *DownloadMedias*, § *PKGBUILD generation*).
Vérification : `just ci`
Commit : `refactor(package): list the eight media assets once, in the on-screen order`

**B3 — Les consommateurs lisent la table**
Description : `check_media_changes`, `medias_to_sha1_map`, `handle_download_medias` et
`candidate_from` passent par `Medias::iter()` / `MEDIA_KINDS` ; `CANDIDATE_MEDIA` et la
bascule `video-normalized` dupliquée disparaissent.
Vérification : `just ci`
Commit : `refactor(pipeline): read the media list from the shared table`

**B4 — Le format des chemins de `description.xml`**
Description : test de régression d'abord (format hostile → le chemin XML doit désigner le
fichier réellement écrit), vu rouge, puis `make_game()` passe par `media_filename()`.
Test et correctif dans le **même** commit (§5).
Vérification : `just ci`
Commit : `fix(package): name the description.xml assets like the files on disk`

**B5 — `rom_unchanged` en un seul endroit**
Description : une fonction pure `(entry, sha1, extras) -> (Option<bool>, String)` rendant
la décision et sa ligne de debug, appelée par `ComputeHashes` et `LookupSS`. Tests : pas
d'entrée d'état, sha1 identique, sha1 différent, nombre de disques différent, sha1 de
disque 2 différent, sha1 courant vide.
Vérification : `just ci`
Commit : `refactor(pipeline): decide rom_unchanged in one place`

**B6 — La boucle des disques**
Description : `CopyRom` et `DownloadRom` partagent le parcours (répertoire, sortie
anticipée sur `rom_unchanged`, disque 1 puis extras) ; ce qui les sépare — vérifier un
sha1 local contre l'attendu, ou déléguer à `Download::verify_sha1` — passe par une closure.
Vérification : `just ci`
Commit : `refactor(pipeline): share the disc loop between the copy and download handlers`

#### Phase C — Clôture

**C1 — Roadmap**
Description : `TODO.md` — les deux points cochés, avec ce qui a été trouvé en chemin (les
deux ordres, la double bascule `video-normalized`, le décalage `format`).
Commit : `docs: record the dedup and concurrency batch in the roadmap`

---

### Portes de qualité

- [ ] `just ci` passe à **chaque** commit
- [ ] Tests ajoutés pour tout code pur touché (A1, A2, A3, B1, B4, B5)
- [ ] Doc synchronisée dans le commit qui la concerne
- [ ] Commits atomiques, scope réel, sujets de qualité changelog
- [ ] Branche `refactor/v0.21.0`, non mergée par Claude
- [ ] `manual_tests.md` exécuté avant de proposer la release
