# Handoff : refonte de l'UI ratatui de rompom (turn 4)

## Overview

rompom (`github.com/gfriloux/rompom`, branche `master`) transforme des ROMs en paquets
installables. Son TUI actuel (`src/ui/`) découpe l'écran en trois zones : un log
`Completed` en haut, puis deux panneaux `Discovery` et `Downloads` en bas, alimentés par
`PANELS` dans `src/ui/mod.rs`.

Cette refonte **supprime la découpe par phase**. Un ROM n'apparaît plus dans deux panneaux
selon son avancement : il occupe **une ligne unique, à sa place d'arrivée dans la file**, et
ce sont des colonnes qui disent où il en est (identification, packaging, ROM, chacun des 9
médias). Objectif exprimé par l'auteur : voir le débit et l'ETA, et lire l'état de n'importe
quel ROM sans le chercher ailleurs.

Le design retenu est le **turn 4** du fichier de maquettes (options `4a`, `4b`, `4c`).

## À propos des fichiers de design

`mockups.dc.html` est une **référence de design réalisée en HTML** : c'est une maquette de ce
que doit afficher le terminal, pas du code à reprendre. Le travail consiste à **recréer ces
écrans en Rust avec ratatui**, dans le code existant de `src/ui/`, en suivant les conventions
du dépôt (indentation 2 espaces, `rustfmt`, `clippy`, pre-commit hook via `nix develop`).

Le HTML simule une grille de cellules monospace : chaque `<div>` est une ligne du terminal,
chaque `<span class="c" style="width:Nch">` est une colonne de N cellules. Les largeurs en
`ch` du HTML se lisent donc directement comme des largeurs en colonnes de terminal.

Ouvrir `mockups.dc.html` dans un navigateur, et regarder la section tout en haut
(`turn 4`) : `4a` écran complet, `4b` vues filtrées + modal, `4c` 80 colonnes + fin de run.
Les turns 1 à 3 en dessous sont l'historique des explorations, conservés pour référence
seulement.

## Fidélité

**Haute fidélité.** Couleurs, largeurs de colonnes, libellés, glyphes et raccourcis clavier
sont définitifs et doivent être repris tels quels. Une seule substitution : les icônes médias
sont affichées en Unicode dans la maquette (`≡ ▶ ▣ ▨ ▤ ◫ ▬ ◎ ❑`) parce que les navigateurs
n'ont pas les glyphes Nerd Font ; **en production, garder les glyphes Nerd Font existants** de
`MEDIA_ICONS` (`󰗚 󰕧 󰋩 󰋫 󰹙 󱂬 󰯃 󰊢 󰂺`), qui occupent la même colonne.

## Écrans

### 1. Écran principal (maquette `4a`)

**But** : suivre un run de bout en bout, repérer les blocages, sélectionner un ROM.

**Layout** — `Layout::vertical` sur `frame.area()` :

| Zone | Contrainte | Contenu |
|---|---|---|
| Bandeau | `Constraint::Length(4)` | bloc `rompom · snes · N roms` |
| Grille | `Constraint::Min(1)` | bloc `roms · ordre d'arrivée · done/total` |
| Aide | `Constraint::Length(1)` | ligne de raccourcis, hors bloc |

Les deux blocs sont des `Block::bordered().border_type(BorderType::Rounded)` avec un titre
en gras — c'est déjà ce que fait `styled_block()` dans `src/ui/render.rs`, à conserver.

**Bandeau (2 lignes de contenu)**

Ligne 1 — progression :
- col 0..10 : libellé `progress`, `DarkGray`
- barre de 40 cellules : `█` en vert pour la part faite, `█` en `#2b323c` pour le reste.
  Un `Gauge` ratatui convient (`gauge_style` fg vert, bg `#2b323c`, `label` vide) ; sinon
  deux `Span` suffisent.
- puis 4 compteurs à pas fixe : `✓ 418 new` (vert) sur 15 col, `= 771 same` (fg normal) sur
  15 col, `✗ 15 failed` (rouge) sur 14 col, `? 1 to id` (jaune).

Ligne 2 — débit :
- libellé `rate`, puis un **sparkline de 30 cellules** avec `▁▂▃▄▅▆▇█` (ratatui a un widget
  `Sparkline`, mais une `Line` de `Span` colorés en cyan suffit et évite un buffer séparé).
  Alimenter avec le nombre de ROMs terminés par intervalle de ~2 s, fenêtre glissante de 30.
- puis `29 rom/min` (chiffre en gras), `12.1 MiB/s`, `eta 4m 20s`, `7/8 workers`.

L'ETA se calcule sur la moyenne des 60 dernières secondes, pas sur la moyenne globale du run :
sinon il ne bouge plus après quelques minutes.

**Grille**

En-tête (une ligne, tout en `DarkGray`), puis un séparateur `─` sur toute la largeur :

| Colonne | Largeur | Contenu |
|---|---|---|
| `#` | 6 | index d'arrivée du ROM dans la file |
| `rom` | 30 | nom scrapé, ou nom de fichier tant qu'il n'est pas identifié |
| `id` | 5 | état de l'étape identification |
| `pkg` | 5 | état de l'étape PKGBUILD |
| `rom` | 6 | état du téléchargement ROM (`✓`, `62%`, spinner, `✗`) |
| médias | 27 | 9 pastilles, une par entrée de `MEDIA_ICONS`, séparées par 2 espaces |
| `temps` | 10 | temps écoulé sur ce ROM, `—` s'il n'a pas démarré |
| `état` | reste | phrase d'état courante |

Aucune largeur ne doit être égale à son contenu le plus long : garder 2 cellules de marge,
sinon les colonnes se touchent.

Alphabet des cellules d'étape (`id`, `pkg`, `rom`) :

| Glyphe | Sens | Couleur |
|---|---|---|
| `·` | pas encore atteint | `#2b323c` |
| spinner `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` | en cours | cyan `#5ec8d8` (jaune si attente utilisateur) |
| `62%` | téléchargement en cours, avancement connu | vert `#78d18b` |
| `✓` | fait | vert |
| `=` | inchangé depuis le dernier run | `DarkGray` |
| `✗` | échec | rouge `#e86a76` |

Alphabet des pastilles médias :

| Glyphe | Sens | Couleur |
|---|---|---|
| `●` | récupéré maintenant | vert |
| `●` | déjà à jour | `DarkGray` |
| `○` | absent de ScreenScraper | rouge |
| `◐` | en cours | vert |
| `·` | pas encore tenté | `#2b323c` |

C'est exactement la sémantique des trois `Vec<String>` déjà présents sur `RomEntry`
(`media_found`, `media_unchanged`, `media_missing`) ; le seul ajout est l'état « en cours »,
qui peut se déduire du média nommé dans le statut courant.

**Ordre de la liste** : ordre d'arrivée, jamais retrié. Les ROMs actifs restent à leur place
et sont simplement plus colorés que les terminés. La liste défile toute seule pour garder la
zone active visible ; une ligne de pied indique ce qui est hors champ :
`↑ 1197 plus haut · 181 en file ↓`, suivie de la légende des pastilles.

**Ligne de détail** : le ROM sélectionné est surligné (fond `#161c24`, préfixe `▌` dans la
colonne nom) et **trois lignes de détail s'insèrent juste en dessous**, sur le même fond :

```
        fichier     Ganbare Goemon 2 (J) [!].sfc      1.5 MiB     source archive.org/snes_complete_us
        sha1        3f9a1c77e04b2d8815ce6f0aa19b7c4d2e5081aa      aucun hit hash sur screenscraper
        recherche   4 candidats par nom · meilleur 91%            enter ouvrir le modal
```

Colonnes : 6 vides, libellé sur 12, valeur sur 34 puis 14, reste libre. Le contenu des
lignes 2 et 3 dépend de la phase : pour un ROM en téléchargement, montrer plutôt l'URL
source, la taille et le débit instantané ; pour un échec, la cause et le nombre d'essais.

### 2. Vue filtrée « erreurs seules » (maquette `4b`)

Même bandeau masqué, un seul bloc bordé rouge `#e86a76`, titre
`filtre : erreurs · 15 sur 1389`. Colonnes : `#` 6, `rom` 30, `id` 5, `pkg` 5, `rom` 6,
`cause` 26, `tentatives`. Pied de bloc : le décompte par cause
(`9 sha1 · 4 absents · 2 http`) puis les actions `r` relancer la sélection, `R` tout
relancer, `w` écrire `snes.errors.log`, `esc` retour.

### 3. Vue filtrée « à identifier » (maquette `4b`)

Bloc bordé jaune `#e2b35c`, titre `à identifier · N en attente`. Colonnes : `#` 6,
`fichier` 38, `attente` 12 (depuis combien de temps le worker est bloqué), `candidats` 14,
`meilleur score` (vert ≥ 90 %, jaune sinon, rouge si aucun candidat). La ligne sélectionnée
est surlignée sur fond `#1c1a14`. Actions : `enter` ouvrir, `a` accepter tous les ≥ 90 %,
`s` passer, `esc` retour.

`a` est un ajout fonctionnel par rapport au code actuel : il envoie un `ModalResponse::
SelectedId` pour chaque ROM en attente dont le meilleur candidat dépasse 90 %, sans ouvrir le
modal. Le seuil doit rester visible dans le libellé.

### 4. Modal d'identification (maquette `4b`)

Reprend `render_modal()` mais aligne ses colonnes sur celles de la grille, pour que les
médias disponibles d'un candidat se lisent avec le même alphabet que le reste de l'écran.

Bloc bordé jaune, titre `identifier · <#> · <fichier>`. Contenu :
1. ligne `sha1` : libellé sur 10, hash sur 44, puis taille et `aucun hit hash`
2. ligne vide
3. en-tête de tableau : `candidat` 36, `année` 7, `id` 9, médias 27, `score`
4. les candidats, ligne sélectionnée préfixée `▶` et surlignée `#1c1a14`
5. ligne vide, puis `sélection` : éditeur, genre, nombre de joueurs, région, nombre de médias
6. ligne de raccourcis

Les médias par candidat demandent un appel `jeu_infos` par candidat. Si c'est trop coûteux,
ne les remplir que pour le candidat sélectionné (les autres restent en `·`) — la colonne doit
exister dans tous les cas.

Modes `Input` et `Confirming` : conserver le comportement actuel de `src/ui/modal.rs`
(saisie d'un ID, `Looking up…`, confirmation), en les affichant dans le même bloc plutôt
qu'en remplaçant la liste.

### 5. Vue 80 colonnes (maquette `4c`)

En dessous de 100 colonnes, replier la grille : supprimer les colonnes `#` et `temps`,
réduire `rom` à 26, `id`/`pkg` à 4, `rom` à 5, resserrer les médias à 20 (un seul espace
entre pastilles), et raccourcir les libellés d'état (`scrap`, `rom`, `8/9`, `file`,
`id · m`). Le bandeau perd ses compteurs nommés et devient deux lignes compactes.

### 6. Fin de run (maquette `4c`)

La grille reste affichée telle quelle ; seul le bandeau change :
- bordure verte, titre `run terminé · snes · N roms · 41m 12s`
- ligne `résultat` : barre où la part rouge des échecs est visible en fin de barre, puis les
  trois compteurs
- ligne `débit` : le sparkline complet du run, débit moyen, volume total
- puis un bloc `couverture médias · N identifiés` : deux colonnes de 5 et 4 lignes, chaque
  ligne étant icône, nom du média, barre de 20 cellules, pourcentage. Barre verte au-dessus
  de 50 %, jaune en dessous.
- enfin deux lignes de sortie : les échecs par cause avec `e` / `R`, et les commandes de
  suite (`cd snes && makepkg`, `repo-add …`), `q` pour quitter.

Ce bloc remplace l'actuel `Summary::print()` de `src/summary.rs`, qui écrit après la sortie
du TUI. Le bilan est désormais affiché **dans** l'alternate screen, et l'utilisateur quitte
avec `q`. Garder `Summary::print()` en secours pour les sorties non-interactives.

## Interactions

| Touche | Effet |
|---|---|
| `↑` `↓` | déplace la sélection dans la grille |
| `g` / `G` | début / fin de liste |
| `enter` | déplie le détail ; sur un ROM à identifier, ouvre le modal |
| `f` | cycle de filtres : tout → actifs → erreurs → à identifier |
| `e` | vue erreurs |
| `m` | vue à identifier (le compteur est affiché dans l'aide) |
| `r` / `R` | relance la sélection / tous les échecs |
| `w` | écrit `snes.errors.log` |
| `esc` | quitte la vue filtrée ou le modal |
| `q` | quitte après la fin du run |
| `ctrl-c` | interrompt et sauve `<system>.run.yml` (comportement actuel conservé) |

Le suivi automatique du défilement se désactive dès que l'utilisateur bouge la sélection, et
se réactive sur `G`. Le modal reste bloquant comme aujourd'hui (`show_modal` dans le thread
de rendu), les autres workers continuent.

## État

Ce que `AppState` doit gagner par rapport à `src/ui/mod.rs` actuel :

- `selected: Option<usize>` — index dans `roms`, l'ordre d'arrivée est déjà celui du `Vec`
- `scroll: usize` et `follow: bool`
- `filter: Filter` — `All | Active | Errors | Unidentified`
- `rate: VecDeque<(Instant, usize)>` — fenêtre glissante pour sparkline, débit et ETA
- `bytes_total: u64`, `bytes_window: VecDeque<(Instant, u64)>` pour les MiB/s
- sur `RomEntry` : `started_at: Option<Instant>`, `finished_at`, `bytes_done`/`bytes_total`
  pour le pourcentage de la colonne `rom`, `error: Option<ErrorKind>` (sha1 / absent / http),
  `attempts: u8`, `pending_media: Option<String>` pour la pastille `◐`
- pour la vue à identifier : `waiting_since: Instant`, `candidate_count`, `best_score`

`RomPhase` peut disparaître au profit d'un état par étape, puisque plus rien ne range les
ROMs par phase. `PANELS` et `render_active()` disparaissent avec la découpe.

## Tokens

Palette (truecolor, `Color::Rgb`) :

| Rôle | Hex |
|---|---|
| fond | `#0e1116` |
| fond de ligne sélectionnée | `#161c24` (jaune : `#1c1a14`) |
| bordures, cellules vides | `#2b323c` |
| texte | `#c9d1d9` |
| texte secondaire | `#5b6673` |
| accent / en cours | `#5ec8d8` |
| succès | `#78d18b` |
| attente, retry | `#e2b35c` |
| erreur | `#e86a76` |

Spinner : `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`, 80 ms par image (constantes déjà présentes).
Sparkline : `▁▂▃▄▅▆▇█`. Barres : `█`. Séparateur : `─`. Curseur de sélection : `▌`.

Prévoir un repli 16 couleurs si `COLORTERM` n'annonce pas `truecolor` : cyan, vert, jaune,
rouge, gris foncé — c'est la palette actuelle du projet, la correspondance est directe.

## Assets

Aucun. Les seuls glyphes externes sont ceux de `MEDIA_ICONS`, qui exigent une Nerd Font dans
le terminal ; c'est déjà le cas aujourd'hui.

## Fichiers

- `mockups.dc.html` — les maquettes. Section du haut = `turn 4`, la cible. Les sections
  `turn 3`, `turn 2`, `turn 1` en dessous sont l'historique (autres découpes explorées,
  écrans de démarrage, de reprise, et d'erreur ScreenScraper).

Fichiers du dépôt concernés :
- `src/ui/render.rs` — réécrit : bandeau, grille, détail, vues filtrées, bilan
- `src/ui/mod.rs` — `AppState`, `RomEntry`, suppression de `PANELS` / `RomPhase`, gestion des
  touches
- `src/ui/modal.rs` — nouvelles colonnes, mode « accepter les ≥ 90 % »
- `src/summary.rs` — devient un repli non-interactif
