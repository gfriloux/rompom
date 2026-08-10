# Tests manuels — v0.18.0

Ce que les tests unitaires ne peuvent pas couvrir : rendu ratatui, vrais appels réseau,
tty, `makepkg`. Enrichi au fil du développement, **exécuté avant la release** ; le
résultat réel est consigné ici (y compris « non joué », avec la raison).

Convention : `☐` à faire · `☑` passé · `☒` échoué · `⊘` non joué

---

## §1 — Hachage (lot A, étape A4 seulement)

À ne jouer que si A4 (sortie de `checksums`) est retenue.

- ☐ **1.1 — Un run complet ne voit rien changer.** Sur un système déjà scrapé avec un
  `state.yml` à jour, relancer rompom. Attendu : **toutes** les ROMs en `=` (gris,
  `package_unchanged`), aucun `pkgver` bumpé, aucun téléchargement. Un seul `✓` vert
  signifie que le nouveau sha1 ne rend pas la même valeur que l'ancien — bloquant.
- ☐ **1.2 — Croisement avec `sha1sum`.** Sur trois ROMs (une petite, une > 100 Mo, une
  `.chd`), comparer le `sha1` écrit dans `state.yml` avec `sha1sum` du système.
- ☐ **1.3 — `makepkg` accepte les `sha1sums`.** Construire un paquet généré et vérifier
  que `makepkg` valide les sommes sans `--skipchecksums`.

## §2 — Ligne de commande (lot D)

- ☐ **2.1 — Argument inconnu.** `rompom --systm snes` → message lisible + usage, **pas**
  de trace de panique, `echo $status` = 2.
- ☐ **2.2 — Système inconnu.** `rompom -s nexiste-pas` → message, `$status` = 1.
- ☐ **2.3 — Système sans `source`.** `rompom -s nes` (le `rompom.yml` d'exemple en a
  plusieurs sans source) → message, `$status` = 1.
- ☐ **2.4 — `--version`** rend la version de `Cargo.toml`.
- ☐ **2.5 — `--list-systems`** liste tous les systèmes avec id et type de source, et
  marque visiblement ceux qui n'ont pas de source.
- ☐ **2.6 — `--help`** mentionne tous les nouveaux drapeaux (`--init`, `--list-systems`,
  `--plain`, `--resume`, `--ascii`, `--version`).

## §3 — `--init` (lot E)

- ☐ **3.1 — Fichier absent.** Dans un `HOME` jetable (`env HOME=(mktemp -d) rompom --init`) :
  le fichier est écrit, et `rompom -s snes` juste après donne l'erreur des credentials
  vides — pas une erreur de parsing.
- ☐ **3.2 — Fichier présent.** Rejouer `--init` : **refus**, le fichier existant est
  intact (comparer le sha1 avant/après), `$status` = 1.
- ☐ **3.3 — `lang` invalide.** Mettre `lang: [fr-FR]` → erreur au chargement qui liste
  les six codes acceptés.

## §4 — Rendu TUI (lots F)

À jouer dans un terminal avec Nerd Font, puis dans un terminal sans.

- ☐ **4.1 — Retries visibles.** Provoquer un échec transitoire (couper le réseau en cours
  de run) : la barre affiche `retrying (1/3)…` puis `(2/3)`, en jaune, et revient à son
  statut normal si la reprise réussit.
- ☐ **4.2 — `--ascii`.** Les 9 pastilles médias sont lisibles sans Nerd Font, l'alignement
  des colonnes tient, la légende du bas suit la même table.
- ☐ **4.3 — Sans `--ascii` et sans Nerd Font** : constater les tofus, et vérifier que le
  README dit quoi faire.
- ☐ **4.4 — Modale d'identification** inchangée : liste, mode saisie (`i`), confirmation,
  Esc, Ctrl-C.

## §5 — Mode non interactif (lot G)

- ☐ **5.1 — Détection de tty.** `rompom -s snes | cat` : sortie ligne par ligne, **aucune**
  séquence d'échappement ni de code couleur dans le pipe, pas de raw mode.
- ☐ **5.2 — `--plain` dans un vrai tty** : même rendu ligne par ligne, le drapeau force.
- ☐ **5.3 — `--resume=yes` / `=no`** avec un `run.yml` présent : aucune question posée,
  le comportement suit le drapeau. Vérifier que `=no` supprime bien `run.yml`.
- ☐ **5.4 — Hors tty sans `--resume`**, `run.yml` présent : run neuf, `run.yml` supprimé,
  et le run **ne bloque pas** en attente de stdin.
- ☐ **5.5 — ROM non identifiée hors tty** : le run continue, la ROM finit en `✗` avec la
  cause « needs manual identification », et elle apparaît dans la section `Failures`.
  **Le code de sortie reste 0** : c'est le choix documenté dans le tableau des codes du
  README — 0 dit « le run a eu lieu », pas « tout a réussi ». Une bibliothèque de 400 ROMs
  dont une seule est non identifiée ferait échouer le job à chaque passage sinon. Si on
  veut l'inverse, c'est une décision à prendre explicitement, pas un effet de bord.
- ☐ **5.6 — Ctrl-C en mode plain.** Le thread de rendu qui captait les touches n'existe
  plus : c'est le handler `SIGINT` qui doit prendre le relais. Vérifier qu'un Ctrl-C
  arrête proprement, écrit `run.yml`, et qu'un second Ctrl-C sort tout de suite.
- ☐ **5.7 — Run complet en CI simulé** : `rompom -s snes --plain --resume=no < /dev/null`
  dans un environnement sans `TERM`, jusqu'au bout, `$status` = 0.

## §6 — Non-régression de bout en bout

- ☐ **6.1 — Multi-disc réel.** Un jeu Saturn ou PSX 2 disques : un seul paquet, `.m3u`
  correct, les deux disques présents, `game.path` pointant sur le `.m3u`.
- ☐ **6.2 — Multi-disc à régions différentes** (C1) : `Game (Disc 1) (USA)` +
  `Game (Disc 2) (Europe)` restent **deux paquets**.
- ☐ **6.3 — Amiga CD32** (C3) : un fichier `(CD32)` reste un paquet simple.
- ☐ **6.4 — `makepkg`** sur un paquet généré, installé sur Batocera, jeu lancé depuis
  EmulationStation avec sa jaquette et sa description.
- ☐ **6.5 — Résumé de fin** : compte total / succès / inchangés / erreurs cohérent avec
  le panneau Completed, section `Failures` non tronquée.

---

## Résultats — exécution du 2026-08-10

Environnement : dépôt de dev, `nix develop`, `XDG_CONFIG_HOME` sur une config jetable avec
deux systèmes (`withsource` sur un dossier vide, `nosource` sans bloc `source`) et des
credentials ScreenScraper factices. **Pas de compte ScreenScraper, pas de Nerd Font, pas de
Batocera, pas de terminal de contrôle** dans cet environnement — d'où les `⊘` ci-dessous.

### Joués et passés

| test | résultat observé |
|---|---|
| 2.1 argument inconnu | `rompom: Unrecognized option: 'systm'` + usage, **exit 2**, pas de panique |
| 2.2 système inconnu | message pointant `--list-systems`, **exit 1** |
| 2.3 système sans source | message disant quoi ajouter, **exit 1** |
| 2.4 `--version` | `rompom 0.17.0` (= `Cargo.toml` au moment du test), **exit 0** |
| 2.5 `--list-systems` | les deux systèmes, id et source ; `nosource` marqué `(no source — cannot be run)` |
| 2.6 `--help` | les 8 drapeaux présents (`--init --list-systems --plain --resume --ascii --version --debug --update-config`) |
| 3.1 `--init` sur config absente | fichier écrit, message d'instruction, **exit 0** |
| 3.2 `--init` sur config présente | refus nommant le chemin, **exit 1**, sha1 du fichier **inchangé** |
| 3.3 `lang` invalide | `unsupported language code fr-FR … ScreenScraper serves synopses in: de, en, es, fr, it, pt`, **exit 1** |
| 5.1 stdout redirigé | **0 séquence d'échappement** dans le fichier de sortie |
| 5.2 `--plain` forcé | idem |
| 5.3b `--resume maybe` | `--resume expects yes or no, got "maybe"`, **exit 2** |

5.1 et 5.2 sortent en 1 : le run va jusqu'à `ScreenScraper::new`, qui refuse les
credentials factices. C'est le chemin voulu — `Ui::new` a bien été traversé en mode plain.

### Trouvé en jouant les tests

**`enable_raw_mode()` échouait déjà avant v0.18** dans cet environnement : il n'y a pas de
terminal de contrôle (`/dev/tty` inouvrable), donc l'`unwrap()` de `Ui::new` paniquait sur
le thread de rendu — et `install_panic_hook()` supprimant la sortie de panique, le run
allait au bout **sans rien afficher et sans rien signaler**. Vérifié en rejouant le même
scénario sur l'état pré-G2. C'est ce qui a fait corriger le commentaire de
`use_plain_output()`, qui annonçait un échec bruyant.

### Non joués — et pourquoi

| test | raison |
|---|---|
| §1 (1.1 → 1.3, hachage) | demande une vraie bibliothèque de ROMs et un `state.yml` d'un run précédent. **Compensé** : le contre-test temporaire a comparé l'ancienne et la nouvelle implémentation sur de vrais fichiers avant la bascule, et `src/hash.rs` garde les vecteurs publiés + un cas multi-chunk. |
| 4.1 → 4.4 (rendu TUI) | pas de terminal de contrôle ici : la TUI ne peut pas s'afficher. `--ascii` et le statut retry sont à relire à l'œil dans un vrai terminal. |
| 5.4 → 5.7 (resume hors tty, Ctrl-C plain, run CI complet) | demandent un `run.yml` d'un vrai run interrompu et des credentials. |
| §6 (bout en bout, `makepkg`, Batocera) | demande compte SS, ROMs, et une installation Batocera. **6.2 et 6.3 sont couverts par les tests unitaires** (`two_regional_releases_do_not_merge_into_one_game`, `two_cd32_games_are_two_packages`) — ce qui reste à vérifier à la main, c'est le `.m3u` réel et le lancement depuis EmulationStation. |

**À jouer par le mainteneur avant de taguer** : 4.1–4.4 dans un terminal avec et sans Nerd
Font, puis 6.1 et 6.4 sur un vrai système multi-disque.
