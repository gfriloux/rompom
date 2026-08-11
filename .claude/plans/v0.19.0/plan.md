## Plan : v0.19.0 — refonte TUI « turn 4 »

**Type :** ui (+ pipeline à la marge)
**Objectif :** solder **P2.7** de `TODO.md` — supprimer la découpe par phase et la
remplacer par une grille unique en ordre d'arrivée, avec bandeau de progression,
sélection dépliable, vues filtrées, modal réaligné, vue repliée et bilan de fin dans
l'écran.
**Pourquoi :** aujourd'hui un ROM apparaît dans deux panneaux selon son avancement et
disparaît du haut de l'écran dès qu'il est terminé. On ne peut ni suivre un ROM précis,
ni voir le débit, ni savoir combien de temps il reste.
**Étage(s) :** `ui`, `pipeline`, `doc`

**Branche :** `feat/v0.19.0`
**Roadmap :** `TODO.md` P2.7 (+ ferme l'item P3 « fuite de permit `modal_sem` »)
**Spec :** `design/tui/handoff.md` + `design/tui/mockups.dc.html` (section `turn 4`)

---

### Périmètre

**In scope**

- Écran principal `4a` : bandeau (progression, compteurs, sparkline, débit, ETA,
  workers actifs) + grille une ligne par ROM + ligne d'aide.
- Sélection : `↑↓`, `g`/`G`, surlignage, trois lignes de détail, suivi automatique du
  défilement qui se coupe dès que l'utilisateur bouge.
- Vues filtrées `4b` : erreurs (`e`), à identifier (`m`), cycle `f`, retour `esc`.
- `w` — écrit `<system>.errors.log`.
- Modal `4b` : colonnes alignées sur la grille, pastilles médias par candidat, ligne
  `selection`, modes `Input` / `Confirming` conservés dans le même bloc.
- Vue repliée `4c` sous 100 colonnes.
- Bilan de fin `4c` dans l'alternate screen, `q` pour quitter.
- Palette truecolor + repli 16 couleurs.
- Suppression de `RomPhase`, `PanelDef`, `PANELS`, `PANEL_HEIGHT`, `render_active()`,
  `render_panel()`, `render_completed()`, `CompletedEntry`.

**Out of scope — et pourquoi**

- **`62 %` dans la colonne `rom`, débit instantané, `rom 2.4/3.9 MiB`.** Ni
  `internetarchive` ni `screenscraper` n'exposent la progression d'un téléchargement
  (cf. `phase0_results.md` §2.1). Un handoff est écrit vers chacun des deux dépôts ;
  la colonne affiche un spinner en attendant. Le **volume total** et le **débit moyen**
  restent affichés : ils se calculent à la granularité du fichier terminé, sans lib.
- **`r` / `R` — relancer les échecs.** Ce n'est pas de l'UI : il faut ré-armer le DAG
  (steps `Failed`/`Skipped` remis à `Pending`), ré-incrémenter `remaining` sans casser
  l'invariant anti-underflow, repousser dans la queue — et après la fin du run, la
  queue est arrêtée et les workers joints, donc il faudrait relancer un pool. Part en
  P3 avec son propre plan. `w` reste : écrire le log des échecs est trivial et couvre
  le besoin immédiat (relancer `rompom -s snes` refait exactement les ROMs échoués,
  puisque leur `state.yml` n'a rien enregistré).
- **`a` — accepter tous les candidats ≥ 90 %.** Le pourcentage n'existe pas
  (`phase0_results.md` §2.2). Remplacé par une colonne `rank`.
- Toute la dette P3 restante hors `modal_sem`.

---

### État du working tree

Propre, `master` @ `2c7888f`, `just ci` vert, 103 tests (`phase0_results.md`).
Rien à faire disparaître avant de commencer.

Ce qui **doit** avoir disparu à la fin : `RomPhase`, `PanelDef`, `PANELS`,
`PANEL_HEIGHT`, `render_active`, `render_panel`, `render_completed`, `completed_item`,
`CompletedEntry`, `status_style`, `centered_rect`, `WorkerContext::modal_sem`.

---

### Fichiers touchés

- [ ] `src/ui/mod.rs` — état, `RomBar`, touches, cycle de vie du bilan
- [ ] `src/ui/render.rs` — réécrit
- [ ] `src/ui/modal.rs` — colonnes, parking des demandes
- [ ] `src/ui/palette.rs` — **nouveau** : jetons de couleur + repli 16 couleurs
- [ ] `src/ui/grid.rs` — **nouveau** : calcul des colonnes, fenêtre de défilement, format
- [ ] `src/ui/rate.rs` — **nouveau** : fenêtre glissante, débit, ETA, sparkline
- [ ] `src/summary.rs` — repli non-interactif, inchangé sur le fond
- [ ] `src/worker/handlers/discovery.rs` — `ModalCandidate` enrichi, cellules `id`
- [ ] `src/worker/handlers/packaging.rs` — cellule `pkg`
- [ ] `src/worker/handlers/downloads.rs` — cellule `rom`, octets écrits
- [ ] `src/worker/handlers/save_state.rs` — fin de ROM
- [ ] `src/worker/mod.rs` — compteur de workers actifs, `modal_sem` retiré
- [ ] `src/worker/run_state.rs` — `restore_bar_for_resumed_rom` sur le nouveau modèle
- [ ] `src/main.rs` — attente du `q` final, taille du pool bloquant
- [ ] `CLAUDE.md` — section « Terminal UI » réécrite, § threads, § Ctrl-C
- [ ] `README.md` — raccourcis clavier
- [ ] `TODO.md` — P2.7 fermé, `r`/`R` et le `%` descendus en P3
- [ ] `.claude/plans/v0.19.0/handoff_*.md` — les deux handoffs

---

### Décisions techniques

**D1 — L'état d'un ROM devient trois cellules, plus neuf pastilles.**
`RomPhase` disait *où* est le ROM ; la grille demande *où il en est sur chaque étape*.
D'où `Cell { Todo, Running, Done, Unchanged, Failed }` pour `id`/`pkg`/`rom` et
`Dot { Todo, Running, Fresh, Unchanged, Missing }` pour les neuf médias. Les trois
`Vec<String>` actuels (`media_found`/`media_unchanged`/`media_missing`) deviennent un
`[Dot; 9]` indexé par `media_icons()` — c'est la même information, mais dans l'ordre
canonique et sans recherche linéaire à chaque frame.

**D2 — `completed` disparaît, `roms` fait foi.**
Aujourd'hui un ROM terminé est copié dans `AppState::completed` : deux sources pour la
même vérité, et le panneau Completed n'existe plus. `Ui::summary()` et `plain_line()`
lisent désormais `roms`. `--plain` continue d'imprimer sa ligne au moment où le ROM
finit, donc son comportement est inchangé.

**D3 — Le `Progress(u8)` de la spec n'est pas ajouté maintenant.**
Une variante que rien ne construit est du code mort, et `TODO.md` P3 en tient déjà la
liste. Elle arrive avec le callback des libs. La colonne garde sa largeur de 6, comme
la spec l'exige, pour que le passage au `62%` ne redécoupe rien.

**D4 — `modal_sem` est supprimé.**
Il sérialise les modals à 1 ; or c'est déjà le cas par construction — un seul thread de
rendu, une seule modale à l'écran. Il empêchait surtout la vue `m` d'exister : à
capacité 1, un seul ROM peut attendre, alors que l'écran en liste plusieurs. Les
demandes sont désormais **parquées** dans `AppState::pending` à leur arrivée et le modal
s'ouvre sur `enter` (ou automatiquement si aucune n'était en attente, pour ne pas changer
le réflexe actuel). `N_BLOCKING_WORKERS` passe de 2 à 8 : un ROM en attente occupe un
worker bloquant, et il en faut assez pour que la file d'attente soit visible.
Bénéfice de bord : l'item P3 « fuite de permit `modal_sem` sur chemin d'erreur → guard
RAII » est fermé en supprimant le semaphore, pas en le gardant.

**D5 — La langue reste l'anglais.**
Les maquettes sont en français, le reste de rompom est en anglais (statuts,
`Summary::print()`, erreurs, README, aide CLI). Les libellés sont traduits, **les
largeurs de colonnes de la spec sont conservées telles quelles** : `time`, `status`,
`queued`, `to identify`, `attempts`, `cause`, `rank`.

**D6 — Les glyphes médias restent ceux de `MEDIA_ICONS`.**
Les maquettes affichent `≡ ▶ ▣ ▨ ▤ ◫ ▬ ◎ ❑` faute de Nerd Font dans un navigateur. En
production, `media_icons()` continue de décider, donc `--ascii` continue de marcher.

**D7 — Tout ce qui est calculable est une fonction pure, dans son module.**
`grid.rs` (largeurs, fenêtre de défilement, troncature), `rate.rs` (débit, ETA,
sparkline), `palette.rs` (résolution des couleurs) n'ont ni `Frame` ni terminal en
paramètre. C'est la seule façon d'avoir des tests sur un lot qui *est* du rendu, dans un
environnement sans terminal de contrôle.

---

### Colonnes (largeurs définitives, relevées sur la maquette)

Grille pleine (≥ 100 colonnes) :

| col | largeur |
|---|---|
| `#` | 6 |
| `rom` | 30 |
| `id` | 5 |
| `pkg` | 5 |
| `rom` | 6 |
| médias | 27 (9 × 3) |
| `time` | 10 |
| `status` | reste |

Grille repliée (< 100 colonnes) : `#` et `time` retirées, `rom` 26, `id` 4, `pkg` 4,
`rom` 5, médias 20 (9 × 2 + 2), `status` reste.

Détail : 6 vides, libellé 12, valeur 34 puis 14 (ligne 1) ou 48 (lignes 2-3).
Vue erreurs : `#` 6, `rom` 30, `id` 5, `pkg` 5, `rom` 6, `cause` 26, `attempts` reste.
Vue à identifier : `#` 6, `file` 38, `waiting` 12, `candidates` 14, `rank` reste.
Modal : `candidate` 36, `year` 7, `id` 9, médias 27, `rank` reste.
Bandeau : libellé 10, barre 40, puis 15 / 15 / 14 / reste.

---

### Étapes atomiques

Chaque étape est un commit qui passe les portes **seul**.

#### Étape 1 : la grille remplace les panneaux par phase
**Description :** nouveau modèle d'état (`Cell`, `Dot`, `RomEntry`, `AppState`),
`RomBar` réécrit sur ce modèle, tous les appelants (`discovery.rs`, `packaging.rs`,
`downloads.rs`, `save_state.rs`, `mod.rs`, `run_state.rs`) adaptés. `render.rs` réécrit :
bandeau minimal (titre, barre de progression, quatre compteurs), grille (en-tête,
séparateur, une ligne par ROM, pied avec ce qui est hors champ + légende), ligne d'aide.
`grid.rs` créé avec les largeurs et la troncature. `PANELS` / `RomPhase` /
`PANEL_HEIGHT` / `render_active` / `render_panel` / `render_completed` / `CompletedEntry`
supprimés. `Ui::summary()` et `plain_line()` lisent `roms`.
**Tests :** largeurs de colonnes pleine largeur, troncature des noms, `truncate_cause`
conservé, mapping cellule → glyphe.
**Vérification :** `just ci` + `--plain` sur un système réel (sortie identique à v0.18).
**Commit :** `feat(ui): show one line per ROM in arrival order`

#### Étape 2 : sélection et détail
**Description :** `selected` / `scroll` / `follow` dans `AppState`, touches `↑` `↓`
`g` `G`, surlignage `▌` + fond, trois lignes de détail sous la sélection, pied
`↑ N above · N queued ↓`. `RomEntry` gagne `file_name`, `size`, `sha1`, `source`,
`candidates` — alimentés à la collecte et par `LookupSS`.
**Tests :** `scroll_window()` (fenêtre, suivi actif/inactif, listes plus courtes que
l'écran, sélection en tête et en queue).
**Vérification :** `just ci` + relecture visuelle (manual_tests.md §2)
**Commit :** `feat(ui): unfold the selected ROM's details`

#### Étape 3 : débit, volume et ETA
**Description :** `rate.rs` — fenêtre glissante de 30 échantillons de ~2 s, sparkline
`▁▂▃▄▅▆▇█`, ROM/min, MiB/s, volume, ETA sur les 60 dernières secondes (pas sur la
moyenne du run, sinon il se fige). Compteur de workers actifs (`AtomicUsize` dans
`WorkerContext`, incrémenté autour de `execute_step`). Les handlers de téléchargement
remontent la taille du fichier écrit.
**Tests :** `rate_per_min()`, `eta()` (dont division par zéro et fenêtre vide),
`sparkline()`, `format_bytes()`, `format_elapsed()`.
**Vérification :** `just ci`
**Commit :** `feat(ui): show run throughput, volume and ETA`

#### Étape 4 : vue erreurs
**Description :** `Filter { All, Active, Errors, Unidentified }`, touches `f` (cycle),
`e`, `esc`. Bloc bordé rouge, colonnes `cause` / `attempts`, pied avec le décompte par
cause. Classification `ErrorKind` (sha1 / absent / http / other) déduite de la cause.
`attempts` remonté par `execute_step`.
**Tests :** `error_kind()` sur les causes réellement produites par le pipeline,
décompte par cause.
**Vérification :** `just ci`
**Commit :** `feat(ui): filter the grid on failed ROMs`

#### Étape 5 : `w` écrit le journal des échecs
**Description :** `w` dans la vue erreurs écrit `<system>.errors.log` dans le répertoire
courant, une ligne par échec, cause **non tronquée**. Confirmation affichée dans la
ligne d'aide.
**Tests :** contenu du journal (fonction pure `errors_log(&[…]) -> String`).
**Vérification :** `just ci`
**Commit :** `feat(ui): write <system>.errors.log from the errors view`

#### Étape 6 : le thread de rendu sérialise les modals, `modal_sem` disparaît
**Description :** les `ModalRequest` sont parquées dans `AppState::pending` au lieu
d'ouvrir immédiatement une modale bloquante ; le modal s'ouvre sur `enter`, ou tout de
suite si rien n'attendait. `WorkerContext::modal_sem` supprimé (et avec lui la fuite de
permit listée en P3). `N_BLOCKING_WORKERS` 2 → 8.
**Tests :** aucun test pur possible (canaux + terminal) ; couvert par `manual_tests.md`.
**Vérification :** `just ci` + test manuel de la modale
**Commit :** `refactor(pipeline): let the render thread serialise identification modals`

#### Étape 7 : vue à identifier
**Description :** touche `m`, bloc bordé jaune, colonnes `waiting` / `candidates` /
`rank`, `enter` ouvre le modal du ROM sélectionné, `s` répond `Cancelled`, `esc` revient.
Le compteur apparaît dans la ligne d'aide de la grille.
**Tests :** `format_waiting()` ; le reste est du rendu.
**Vérification :** `just ci` + test manuel
**Commit :** `feat(ui): filter the grid on ROMs awaiting identification`

#### Étape 8 : modal réaligné
**Description :** `ModalCandidate` gagne `media: [Dot; 9]`, `editor`, `genre`,
`players`, `region`, `rank` — tous remplis dans `discovery.rs` depuis les `JeuInfo` que
`jeu_recherche` renvoie déjà, **sans appel supplémentaire**. Modal redessiné aux
colonnes de la grille, ligne `selection`, `Input` / `Confirming` dans le même bloc.
**Tests :** projection `JeuInfo` → `ModalCandidate` (fonction pure, sur une fixture
sans credentials).
**Vérification :** `just ci` + test manuel
**Commit :** `feat(ui): align the identification modal on the grid columns`

#### Étape 9 : vue repliée sous 100 colonnes
**Description :** `grid.rs` choisit le jeu de colonnes selon la largeur ; libellés de
statut courts (`scrap`, `rom`, `8/9`, `queued`, `id · m`) ; bandeau compact sur deux
lignes.
**Tests :** `grid_columns(80)` vs `grid_columns(120)`, et le seuil exact à 99/100.
**Vérification :** `just ci`
**Commit :** `feat(ui): fold the grid below 100 columns`

#### Étape 10 : bilan de fin dans l'interface
**Description :** en fin de run le bandeau devient vert (`run finished · <system> ·
N roms · durée`), une ligne `result`, une ligne `throughput`, un bloc
`media coverage` sur deux colonnes, puis les deux lignes de sortie. `q` quitte.
`main.rs` appelle `ui.finish_run(summary)` puis `ui.wait_for_quit()`. `Summary::print()`
reste le repli `--plain` et le chemin d'un run interrompu (Ctrl-C ne montre pas de
bilan : il y a un `run.yml` à annoncer).
**Tests :** répartition du bloc couverture en deux colonnes (5 + 4).
**Vérification :** `just ci` + `--plain` inchangé
**Commit :** `feat(ui): show the end-of-run report inside the interface`

#### Étape 11 : palette et repli 16 couleurs
**Description :** `palette.rs` — les neuf jetons du handoff en `Color::Rgb`, repli sur
les couleurs nommées quand `COLORTERM` n'annonce pas `truecolor`. Un seul point de
décision, lu une fois au démarrage.
**Tests :** résolution des jetons dans les deux modes.
**Vérification :** `just ci`
**Commit :** `feat(ui): fall back to 16 colors without truecolor`

#### Étape 12 : handoffs vers les libs
**Description :** `handoff_internetarchive_download_progress.md` et
`handoff_screenscraper_media_progress.md` dans `.claude/plans/v0.19.0/` — ce qui manque,
où, la forme d'API souhaitée, et ce que rompom activera au retour (variante
`Cell::Progress`, débit instantané, ligne de détail).
**Vérification :** relecture
**Commit :** `docs: hand off download progress reporting to internetarchive and screenscraper`

#### Étape 13 : roadmap
**Description :** `TODO.md` — P2.7 coché avec ce qui a réellement été livré et ce qui
ne l'a pas été ; `r`/`R` et le `62 %` descendus en P3 ; l'item `modal_sem` fermé.
**Vérification :** relecture
**Commit :** `docs: record what v0.19.0 closed in the roadmap`

#### Étape 14 : release
**Description :** `just release 0.19.0`, relecture du changelog généré.
**Vérification :** `just ci`, `nix build`, `just changelog-preview`
**Commit :** `chore(release): v0.19.0`

---

### Portes de qualité

- [ ] `just ci` passe à chaque étape
- [ ] `nix flake check` et `just audit` avant la release
- [ ] Tests ajoutés pour tout le code pur introduit (`grid.rs`, `rate.rs`, `palette.rs`)
- [ ] Doc synchronisée dans le même commit que le code
- [ ] Commits atomiques, scope réel (`ui`, `pipeline`, jamais `all`)
- [ ] Branche `feat/v0.19.0`, non mergée par Claude
- [ ] **`manual_tests.md` exécuté par le mainteneur** — le rendu ne peut pas être
      vérifié depuis cet environnement (pas de `/dev/tty`)
