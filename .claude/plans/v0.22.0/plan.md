## Plan : v0.22.0 — relancer les ROMs en échec depuis l'écran de bilan (`r` / `R`)

**Type :** pipeline + ui
**Objectif :** solder le point P3 « `r` / `R` — relancer les ROMs en échec depuis la TUI »,
descendu de P2.7 lors de la refonte TUI.
**Pourquoi :** aujourd'hui, une ROM qui échoue sur un miroir capricieux au numéro 12 d'une
bibliothèque de 1400 n'a qu'un recours : relancer `rompom -s <system>` en entier. Le run
refait la collecte, la métadonnée IA, et repasse sur les 1399 autres — chacune en
skip-if-valid, donc sans retéléchargement, mais pas gratuitement non plus. Les steps
échoués, eux, n'ont **rien** écrit dans `state.yml` : tout ce qu'il faut pour les rejouer
est encore en mémoire à la fin du run.
**Étage(s) :** `pipeline`, `ui`, `doc`

**Branche :** `feat/v0.22.0`
**Roadmap :** `TODO.md` P3 — « `r` / `R` — relancer les ROMs en échec depuis la TUI »

---

### Périmètre

**In scope**

- `r` sur l'écran de bilan : relance la ROM sélectionnée si elle a échoué.
- `R` : relance **toutes** les ROMs échouées.
- Les deux touches sont aussi actives dans la vue erreurs (`e`) **quand le run est fini** —
  c'est là qu'on regarde la liste.
- Re-armement du DAG : fonction pure `rearm()`, testée, qui remet les steps `Failed` et
  leur descendance à `Pending` et recalcule leurs `wait_for`.
- `TaskQueue::restart()` : rouvrir la queue après un `shutdown()`.
- `main` : la paire « lancer le pool / rejoindre » devient un tour qui peut se rejouer.
- Remise à zéro de la ligne dans la grille (`RomBar::rearmed()`).
- Pendant le run, `r` / `R` posent un `notice` disant que c'est disponible à la fin —
  pas de touche muette.
- MAJ `CLAUDE.md`, `README.md`, `TODO.md`.

**Out of scope, et pourquoi**

- **Relance en direct pendant le run.** Décision de l'utilisateur, 2026-08-13. Elle
  imposerait de passer `remaining` d'`AtomicUsize` à `Mutex<usize>` : sans ça, la dernière
  ROM peut décrémenter à 0 et couper la queue exactement pendant qu'on ré-arme, et la
  relance part dans une queue morte. Elle est en plus de faible valeur : une ROM `Failed`
  a déjà épuisé son budget de retry (3 à 5 tentatives avec backoff) ou porte un `Fatal`
  — la rejouer trois secondes plus tard rejoue surtout le même échec.
- **Re-identifier une ROM que l'utilisateur a passée au modal.** `ModalResponse::Cancelled`
  ne fait pas échouer de step : la ROM finit normalement avec une cellule `id` rouge et un
  `description.xml` vide. Elle n'a pas d'`error`, donc ni la vue erreurs ni `R` ne la
  voient. Rouvrir le modal après coup est une autre fonctionnalité (il faudrait un pool
  bloquant et une vue « à ré-identifier »).
- **Relancer une ROM réussie** (« refaire celle-là »). Rien ne le demande, et `state.yml`
  la considérerait inchangée de toute façon.
- **Mode `--plain`.** Il n'y a pas d'écran de bilan à presser une touche dessus ;
  `wait_for_end()` rend `Quit` immédiatement, comme `wait_for_quit()` aujourd'hui.

---

### État du working tree

`master` à `0c99661`, propre. Phase 0 verte, 185 tests — cf. `phase0_results.md`.

---

### Ce que le code fait aujourd'hui (constaté)

**Le cycle de vie d'un échec.** `CopyRom` épuise son budget → `execute_step` pose
`Failed(cause)`, `skip_successors()` marque `SaveState` en `Skipped`, `finish_rom()`
décrémente `remaining` et pose `Rom::finished = true`, `do_dispatch()` décrémente le
`wait_for` de `SaveState`. L'autre branche (`DownloadMedias`) finit, décrémente à son tour,
pousse `SaveState`, qui prend le chemin rapide `Skipped` et ne fait rien.

Après ça, pour le DAG folder (`0 ComputeHashes → 1 LookupSS → 2 WaitModal → 3 BuildPackage
→ 4 CopyRom / 5 DownloadMedias → 6 SaveState`) :

| step | statut | `wait_for` |
|---|---|---|
| 0–3 | `Done` | 0 |
| 4 `CopyRom` | **`Failed`** | 0 |
| 5 `DownloadMedias` | `Done` | 0 |
| 6 `SaveState` | `Skipped` | 0 (décrémenté par les deux branches) |

**Ce qui est encore en mémoire**, et qui change tout par rapport au resume de P0.5 : le
`Rom` n'est pas un objet reconstruit depuis un `run.yml`, c'est **le même**. `sha1`, `jeu`,
`medias`, `romname`, `package_unchanged` sont tous là — d'autant plus depuis v0.21, où
`BuildPackage` et `DownloadMedias` **clonent** ce qu'ils lisent au lieu de le `take()`.
Une relance partielle, impossible au resume, est ici la bonne.

**Ce qui bloque la relance**, dans l'ordre :

1. `remaining` est à 0 et `TaskQueue::shutdown()` est irréversible.
2. Les workers sont joints (`for h in handles { h.join() }`, `main.rs:800`).
3. `Rom::finished` est à `true`, donc `finish_rom()` ne re-décrémentera plus rien.
4. Les `wait_for` des steps re-armés sont à 0 alors que leurs prédécesseurs re-armés vont
   les décrémenter à nouveau — laissés tels quels, ils souffleraient le `debug_assert!`
   anti-underflow de v0.21, et en release feraient exactement ce que ce garde-fou décrit :
   `usize::MAX`, successeur jamais poussé, run qui attend pour toujours.

---

### Décisions techniques

#### 1. Le re-armement est une fonction pure sur `&mut [Step]`

Même choix que `skip_successors()` et `restore_finished_pipeline()`, pour la même raison :
un `Rom` demande un `RomBar`, et un `RomBar` demande toute l'interface. Le DAG se teste sur
un `Vec<Step>` construit à la main — `folder_pipeline()` existe déjà dans
`worker::tests`.

```rust
/// Remet un pipeline échoué en état de tourner, et rend les steps prêts à être poussés.
fn rearm(pipeline: &mut [Step]) -> Vec<usize>
```

L'algorithme, en trois temps :

1. **L'ensemble re-armé** = tous les steps `Failed`, plus leur descendance transitive
   (même parcours à ensemble de visités que `skip_successors`, et pour la même raison :
   un garde par statut s'arrêterait sur un `WaitModal` `Skipped`).
2. **Remise à l'état neuf** de chaque step de l'ensemble : `retry_count = 0`,
   `started_at = finished_at = None`, statut `Pending` — **sauf** un `WaitModal` qui
   n'est là qu'en tant que descendant, qui retourne à son défaut `Skipped`. C'est
   `LookupSS` qui le rouvrira s'il rate à nouveau, comme au premier tour.
   Un `WaitModal` **lui-même** `Failed` (l'appel `jeuinfo_by_gameid` d'après-modale a
   cassé) revient bien à `Pending` : ses candidats vivent dans le `data` de `LookupSS`,
   qui n'est pas re-armé et les a toujours.
3. **`wait_for` recalculé** = nombre de prédécesseurs **qui sont dans l'ensemble**. C'est
   la seule formule correcte : un prédécesseur `Done` hors de l'ensemble ne re-tournera
   pas, donc ne décrémentera pas. Sur l'exemple ci-dessus, `SaveState` repart à **1**
   (seul `CopyRom` va le notifier), pas à 2.

Rendu : les indices de l'ensemble dont le `wait_for` recalculé vaut 0.

#### 2. Un tour de pool, pas un pool permanent

`main` extrait la séquence « spawn `n_main` + `N_BLOCKING_WORKERS`, join » dans une
fonction, appelée en boucle. Entre deux tours : `queue.restart()`, `rearm()` sur les ROMs
visées, `ctx.remaining.store(n)` — dans cet ordre, et **avant** de respawner, puisque
aucun worker n'existe à ce moment-là. Zéro synchronisation à inventer : le seul écrivain
est le thread principal.

Le thread de flush (`state.yml` toutes les 30 s) sort de la boucle : il est démarré avant
le premier tour et arrêté après le dernier, donc un run avec relance est couvert de bout
en bout comme un run simple.

Le `ss_sem` est réutilisé tel quel — cf. `phase0_results.md`, il ressort intact d'une fin
de run normale. Le `WorkerContext` aussi, `remaining` étant le seul champ à remettre.

#### 3. `restart()` vide les trois piles

`shutdown()` abandonne délibérément la voie retardée (les steps en backoff sont `Pending`
et `run.yml` les porte). À la réouverture, un worker joint et un `remaining` à zéro
signifient que tout ce qui traîne encore est périmé par construction. `restart()` vide donc
`main`, `blocking` et `delayed` avant de remettre `shutdown = false` : la seule source de
tâches du tour suivant est le re-armement.

#### 4. Ce que la relance ne refait pas

C'est le point qui distingue cette relance du resume, et il faut qu'il reste vrai :

- **`BuildPackage` n'est rejoué que s'il a lui-même échoué.** Un `pkgver` n'est donc jamais
  bumpé deux fois pour le même contenu. Quand il est rejoué, c'est qu'il n'avait rien
  écrit.
- **`LookupSS` n'est rejoué que s'il a lui-même échoué**, donc aucune requête
  ScreenScraper n'est dépensée pour une ROM dont seul le téléchargement a cassé. C'est ce
  qui rend `R` sur 40 ROMs anodin côté quota.
- **`SaveState` est toujours dans l'ensemble** (c'est la feuille, descendante de tout), donc
  l'état est bien écrit au bout d'une relance réussie.

#### 5. La ligne dans la grille

`RomBar::rearmed()` efface `finished_at`, `started_at`, `error`, `unchanged`, remet
`attempts` à 1, le statut à `queued`, et repasse à `Cell::Todo` **les seules cellules
`Cell::Failed`** : `id`/`pkg`/`rom` déjà `Done` ou `Unchanged` disent la vérité, leur step
ne sera pas rejoué. Les pastilles médias sont laissées telles quelles — si
`DownloadMedias` est re-armé il repasse sur les huit et les réécrit toutes.

`started_at` remis à `None` fait repartir l'horloge de la colonne `time` sur la relance,
qui est la durée qu'on regarde à ce moment-là. `done()` compte les `finished_at`, donc il
redescend de 1 par ROM re-armée — `Rate::tick` encaisse (`saturating_sub`).

#### 6. Pas de touche muette

Pendant le run, `r` et `R` posent le `notice` « retry is available once the run has
finished » plutôt que rien : c'est la règle déjà appliquée à `w` dans la vue erreurs.

---

### Fichiers touchés

- [ ] `src/worker/mod.rs` — `rearm()` + tests
- [ ] `src/queue.rs` — `TaskQueue::restart()` + test
- [ ] `src/ui/mod.rs` — `AppState::retry`, `RunEnd`, `wait_for_end()`, `resume_run()`,
      `RomBar::rearmed()`, les touches dans `navigate()`
- [ ] `src/ui/render.rs` — `r` / `R` dans `done_help_line()` et `errors_help_line()`
- [ ] `src/main.rs` — le tour de pool extrait, la boucle, `rearm_roms()`
- [ ] `CLAUDE.md` — section « End of run », la liste des touches, le tableau `RomBar`
- [ ] `README.md` — tableau des touches, section « The interface » / fin de run
- [ ] `TODO.md` — point P3 coché, avec ce qui a été trouvé en chemin

---

### Étapes atomiques

#### Étape 1 : consigner le plan

**Description :** `plan.md`, `phase0_results.md`, `manual_tests.md`.
**Vérification :** —
**Commit :** `chore(plans): plan the v0.22.0 retry of failed ROMs`

#### Étape 2 : un tour de pool réutilisable

**Description :** extraire de `main` la séquence spawn/join dans une fonction, et sortir le
thread de flush de ce qui deviendra la boucle. Aucun changement de comportement : la
fonction est appelée une fois. C'est la préparation qui rend l'étape 3 lisible en diff.
**Vérification :** `just ci` — 185 tests, inchangés.
**Commit :** `refactor(pipeline): make the worker pool a round that can be run again`

#### Étape 3 : la relance, de bout en bout

**Description :** `rearm()` + ses tests, `TaskQueue::restart()` + son test, la boucle de
`main` avec `rearm_roms()`, les touches `r`/`R`, `RunEnd`/`wait_for_end()`/`resume_run()`,
`RomBar::rearmed()`, les deux lignes d'aide, et la doc (`CLAUDE.md`, `README.md`).

Une seule entrée de changelog, donc un seul commit : découpé plus fin, chaque morceau
serait soit du code mort (clippy `-D warnings` refuse une fonction sans appelant), soit
une touche qui ment.

**Vérification :** `just ci` — 192 tests attendus.
**Commit :** `feat(ui): retry failed ROMs from the end-of-run screen with r and R`

#### Étape 4 : tests manuels

**Description :** dérouler `manual_tests.md` sur un vrai système, consigner le résultat.
**Vérification :** les six scénarios passent.
**Commit :** `chore(plans): record the v0.22.0 manual runs`

#### Étape 5 : roadmap

**Description :** cocher le point dans `TODO.md`, avec ce qui a été trouvé en chemin, et
ajouter l'entrée de séquencement.
**Vérification :** —
**Commit :** `docs: close the r/R retry item of the P3 roadmap`

#### Étape 6 : release

**Description :** `just release 0.22.0`, relire le diff du changelog, puis
`just build-static` — le paquet Nix teste en **release**, où les `debug_assert!` ont
disparu, et c'est exactement comme ça que le test d'underflow de v0.21 avait été pris.
**Vérification :** `just ci`, `nix flake check`, `just audit`, `just build-static`.
**Commit :** `chore(release): v0.22.0`

---

### Tests

**Automatisés** (fonctions pures, ni réseau ni terminal) — 7 nouveaux :

| test | ce qu'il tient |
|---|---|
| `rearm_reruns_the_failed_step_and_its_tail` | `CopyRom` échoué → `{CopyRom, SaveState}` re-armés, le reste intact |
| `rearm_counts_only_the_predecessors_that_will_run_again` | `SaveState` repart à `wait_for = 1`, pas 2 : `DownloadMedias` reste `Done` |
| `rearm_puts_wait_modal_back_to_its_default` | `LookupSS` échoué → `WaitModal` revient `Skipped`, pas `Pending` |
| `rearm_keeps_a_failed_wait_modal_runnable` | `WaitModal` échoué lui-même → `Pending`, ses candidats sont dans `LookupSS` |
| `rearm_handles_two_branches_failing_at_once` | `CopyRom` **et** `DownloadMedias` → `SaveState` à 2, deux steps prêts |
| `rearm_on_a_clean_pipeline_changes_nothing` | rien de `Failed` → rien touché, rien à pousser |
| `a_restarted_queue_parks_a_worker_again` | après `shutdown()` puis `restart()`, `pop_main()` bloque au lieu de rendre `None` |

Le budget de retry remis à zéro (`retry_count = 0`) est vérifié dans le premier test :
sans ça, une ROM relancée après cinq tentatives échouerait à la première.

**Non automatisés** — `manual_tests.md` : la TUI n'est pas testable ici (pas de terminal
de contrôle), et les échecs à provoquer demandent un vrai réseau ou un vrai système de
fichiers.

---

### Portes de qualité

- [ ] `just ci` passe à chaque commit
- [ ] Tests ajoutés pour tout le code pur touché
- [ ] Doc synchronisée dans le même commit
- [ ] Commits atomiques, scope réel, sujets de qualité changelog
- [ ] `just build-static` avant le tag (les `debug_assert!` disparaissent en release)
- [ ] Branche dédiée, non mergée par Claude
