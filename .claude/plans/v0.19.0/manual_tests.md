# Tests manuels — v0.19.0 (refonte TUI « turn 4 »)

> Ce lot **est** du rendu. Rien de ce qui suit n'est automatisable depuis
> l'environnement de développement de Claude : `/dev/tty` y est inouvrable, donc la TUI
> ratatui ne peut être ni lancée ni regardée. Ces tests sont à exécuter par le
> mainteneur, sur un vrai terminal, avec une Nerd Font.
>
> Référence visuelle : `design/tui/mockups.dc.html`, section `turn 4`
> (`4a` écran complet, `4b` vues filtrées + modal, `4c` 80 colonnes + fin de run).
>
> **Trois écarts assumés par rapport à la maquette**, faute de données côté API et non
> par manque de temps (cf. `phase0_results.md` §2) : pas de `62 %` dans la colonne `rom`
> (spinner à la place), pas de score de pertinence ni de touche `a`, pas de `r`/`R`.
> Les libellés sont en **anglais**, pas en français comme la maquette : le reste de
> l'outil l'est. Les largeurs de colonnes, elles, sont celles de la spec.

Enrichi au fil du développement, exécuté en validation.

---

## 1. Écran principal (`4a`)

| # | Test | Attendu |
|---|---|---|
| 1.1 | `rompom -s <system>` sur un système d'au moins 200 ROMs, terminal ≥ 120 colonnes | Deux blocs arrondis : bandeau (4 lignes) et grille. Une ligne d'aide en bas, hors bloc. |
| 1.2 | Regarder un ROM du début à la fin | Il **reste à sa ligne**. Ses cellules `id`, `pkg`, `rom` passent de `·` à spinner puis `✓`. Il ne saute jamais d'un panneau à l'autre. |
| 1.3 | Un ROM inchangé depuis le dernier run | `=` gris dans les trois cellules, pastilles médias grises, statut `unchanged`. |
| 1.4 | Un ROM en échec | `✗` rouge sur la cellule qui a cassé, statut = la cause. |
| 1.5 | Compter les pastilles | Neuf, dans l'ordre de `MEDIA_ICONS` (description en premier). |
| 1.6 | `--ascii` | Les neuf pastilles deviennent les lettres ASCII, les colonnes ne bougent pas. |
| 1.7 | Sans Nerd Font | Neuf tofus — pas de décalage de colonnes. |
| 1.8 | Laisser tourner 2 min | La liste défile toute seule pour garder la zone active visible. Le pied indique ce qui est hors champ (`↑ N above · N queued ↓`). |

## 2. Sélection et détail

| # | Test | Attendu |
|---|---|---|
| 2.1 | `↑` `↓` | La sélection bouge, la ligne est surlignée avec `▌` devant le nom. |
| 2.2 | Sélectionner un ROM | Trois lignes de détail s'insèrent **sous** lui, sur le même fond. |
| 2.3 | Bouger la sélection | Le suivi automatique du défilement se coupe : la liste ne saute plus. |
| 2.4 | `G` | Retour en fin de liste, le suivi automatique reprend. |
| 2.5 | `g` | Début de liste. |
| 2.6 | Sélectionner un ROM en téléchargement | Lignes 2-3 = URL source, taille, et non les infos de recherche. |
| 2.7 | Sélectionner un ROM en échec | Ligne 3 = la cause **entière**, celle que la colonne `status` a dû couper. Le nombre d'essais est dans la vue `e`. |

## 3. Bandeau

| # | Test | Attendu |
|---|---|---|
| 3.1 | Regarder la barre | 40 cellules, part faite en vert, reste en gris sombre, pourcentage à droite. |
| 3.2 | Les quatre compteurs | `✓ N new` vert, `= N same` normal, `✗ N failed` rouge, `? N to id` jaune. Leur somme suit le total terminé. |
| 3.3 | Le sparkline | 30 cellules, se remplit progressivement, bouge quand le débit change. |
| 3.4 | ETA | Décroît. Le débrancher du réseau quelques secondes doit le faire remonter (moyenne 60 s, pas moyenne du run). |
| 3.5 | `N/M workers` | Le premier nombre monte et descend, ne dépasse jamais le second. |
| 3.6 | MiB/s et volume | Cohérents avec la taille des ROMs réellement téléchargés (granularité fichier : ils avancent par paliers, pas en continu — c'est attendu tant que les libs n'exposent pas la progression). |

## 4. Vue erreurs (`e`)

| # | Test | Attendu |
|---|---|---|
| 4.1 | `e` avec au moins un échec | Un seul bloc, bordure rouge, titre `errors · N of M`. |
| 4.2 | Colonnes | `cause` et `attempts` à la place des pastilles. |
| 4.3 | Pied | Décompte par cause, le plus gros seau en tête (`9 checksum · 4 screenscraper · 2 download`). |
| 4.4 | `esc` | Retour à la grille, même sélection. |
| 4.5 | `e` sans aucun échec | Vue vide et explicite, pas de bloc vide muet. |
| 4.6 | `w` | `<system>.errors.log` écrit dans le répertoire courant, causes **non tronquées**, une ligne par échec. Confirmation à l'écran. |
| 4.7 | `f` | Cycle `all → active → errors → to identify → all`. |

## 5. Vue à identifier (`m`) et modal

| # | Test | Attendu |
|---|---|---|
| 5.1 | Lancer sur un système avec des ROMs non reconnues | Plusieurs ROMs peuvent attendre **en même temps** (avant : une seule, à cause de `modal_sem`). |
| 5.2 | `m` | Bloc jaune, colonnes `waiting` / `candidates` / `best match`, temps d'attente qui monte. Le dernier est le **nom** du premier candidat renvoyé par SS, pas un score — ScreenScraper n'en renvoie aucun. |
| 5.3 | `enter` sur une ligne | Le modal du bon ROM s'ouvre. |
| 5.4 | Colonnes du modal | `candidate` / `year` / `id` / neuf pastilles / `rank`, alignées sur la grille. `rank` = la place dans l'ordre renvoyé par SS. |
| 5.5 | Pastilles d'un candidat | Reflètent les médias réellement disponibles pour **ce** candidat, sans latence supplémentaire (aucun appel réseau en plus). |
| 5.6 | Ligne `selection` | Éditeur, genre, joueurs, région, nombre de médias du candidat sélectionné. |
| 5.7 | `i` puis un ID | Mode Input, `Looking up…`, puis Confirming — **dans le même bloc**, la liste n'est plus remplacée par une popup séparée. |
| 5.8 | ID inexistant | Erreur en ligne, on reste en Input. **Aucun mot de passe visible.** |
| 5.9 | Couper le réseau puis saisir un ID | Message distinct de « ID inconnu ». **Aucun mot de passe visible.** |
| 5.10 | `s` sur une ligne | Le ROM est passé (`Cancelled`), le worker repart, la ligne quitte la vue. |
| 5.11 | `esc` dans le modal | Retour à la vue, le ROM reste en attente. |
| 5.12 | Ctrl-C dans le modal | Réponse `Cancelled`, arrêt propre, `run.yml` écrit. |

## 6. Vue repliée (`4c`)

| # | Test | Attendu |
|---|---|---|
| 6.1 | Terminal à 80 colonnes | Colonnes `#` et `time` disparues, médias resserrés, rien ne dépasse ni ne s'enroule. |
| 6.2 | Redimensionner de 120 à 80 en cours de run | Bascule sans casse ni panique. |
| 6.3 | Terminal à 99 puis 100 colonnes | Le seuil est bien à 100 : à 99 les colonnes `#` et `time` disparaissent, à 100 elles reviennent. |
| 6.4 | Terminal très étroit (40 colonnes) | Dégradé lisible, pas de panique arithmétique. |
| 6.5 | Terminal très court (10 lignes) | La grille se réduit, le bandeau reste, pas de panique. |

## 7. Fin de run (`4c`)

| # | Test | Attendu |
|---|---|---|
| 7.1 | Laisser un run aller au bout | Le bandeau devient vert : `run finished · <system> · N roms · durée`. La grille reste affichée. |
| 7.2 | Bloc couverture médias | Deux colonnes (5 lignes + 4), barres vertes > 50 %, jaunes en dessous, pourcentages cohérents avec le run. |
| 7.3 | `q` | Sort de l'alternate screen, terminal restauré, prompt propre. **Rien n'est réimprimé après** : le bilan était à l'écran. |
| 7.4 | Ctrl-C pendant le run | **Pas** d'écran de bilan : message `run.yml`, sortie. |
| 7.5 | `q` **pendant** le run | Ne fait rien — seul Ctrl-C arrête un run. |
| 7.6 | `e` sur l'écran de bilan | La vue erreurs s'ouvre encore, et `w` y écrit toujours le journal. |

## 8. Non-régression des acquis v0.18

| # | Test | Attendu |
|---|---|---|
| 8.1 | `--plain` | Une ligne par ROM terminé, puis `Summary::print()`. **Sortie identique à v0.18.** |
| 8.2 | `rompom … \| cat` | Bascule automatique en plain, aucun code d'échappement. |
| 8.3 | `--plain` avec un ROM non identifié | Échec explicite du step, pas de blocage. |
| 8.4 | Statut `retrying (2/3)…` | Toujours visible, en jaune, **pendant** le backoff (P2.3). |
| 8.5 | Ctrl-C puis relancer | Prompt de reprise, `--resume=no` repart de zéro, `--resume=yes` reprend. |
| 8.6 | Un ROM échoué en amont, repris depuis `run.yml` | Réapparaît en **échec**, pas en succès (P1.2). |
| 8.7 | Cause d'échec longue | Tronquée dans la grille, **entière** dans `Summary::print()` et dans `errors.log`. |
| 8.8 | Un handler qui panique | Le run continue, le ROM est en échec, l'interface ne gèle pas (P0.3). |

## 9. Terminaux

| # | Test | Attendu |
|---|---|---|
| 9.1 | `COLORTERM=truecolor` | Palette exacte du handoff. |
| 9.2 | `COLORTERM=` (vidé) | Repli 16 couleurs : cyan, vert, jaune, rouge, gris. Lisible, rien d'invisible. La ligne sélectionnée n'a **plus de fond** — c'est voulu — et se repère au `▌` et au nom en gras. |
| 9.3 | `TERM=xterm-256color` dans tmux | Pas de couleur aberrante. |
| 9.4 | Thème clair | Le fond `#0e1116` ne rend pas le texte illisible (ou : le fond n'est pas forcé). |

---

## Ce qui a été joué ici — et ce qui ne pouvait pas l'être

Exécuté le 2026-08-11 contre le binaire statique de la release
(`nix build` → `result/bin/rompom`, **0.19.0**, 11 481 424 octets, contre 11 609 000 en
v0.18 : la découpe par phase pesait ~128 Kio).

### Joués et verts

| test | résultat |
|---|---|
| `--version` | `rompom 0.19.0` — répondu avant toute lecture disque, **exit 0** |
| `--list-systems` | nom, id et source ; **exit 0** |
| système inconnu | `system "nope" is not in rompom.yml — run --list-systems to see what is`, **exit 1** |
| 8.1 / 8.2 — `--plain` et stdout redirigé | **0 séquence d'échappement** dans la sortie ; le message d'erreur part sur stderr, **exit 1** |
| 5.3b — `--resume maybe` | `--resume expects yes or no, got "maybe"`, **exit 2** |
| portes | `just ci` (146 tests), `nix flake check`, `just audit` (4 avertissements autorisés, inchangés), `nix build` |

Les deux runs `--plain` sortent en 1 sur le dossier source inexistant : c'est le chemin
voulu de P0.6, et il prouve que `Ui::new` a bien été traversé en mode plain sans que rien
ne s'affiche ni ne panique.

### Non joués — et pourquoi

| test | raison |
|---|---|
| §1 à §7 (tout le rendu) | **pas de terminal de contrôle ici** : `/dev/tty` est inouvrable, la TUI ratatui ne peut être ni lancée ni regardée. C'est l'essentiel de ce lot. |
| §5 (modal, identification) | demande en plus un compte ScreenScraper et des ROMs que SS ne reconnaît pas. |
| §8.4 à §8.8 (retry, reprise, panique) | demandent un vrai run, un `run.yml`, des credentials. |
| §9 (terminaux, palettes) | même raison que §1 à §7. |

**Tout §1 à §7 et §9 est donc à jouer par le mainteneur avant de taguer.** Les fonctions
pures qui portent ces écrans sont couvertes — `ui/grid.rs`, `ui/rate.rs`, `ui/errors.rs`,
`ui/palette.rs` et `candidate_from()` ont 43 des 146 tests — mais aucun test ne dit à quoi
l'écran ressemble.
