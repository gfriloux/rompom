# Phase 0 — audit du dépôt avant de coder (v0.19.0)

Exécuté le 2026-08-10, avant toute modification.

## 1. Portes de qualité

```
nix develop --command just ci
```

**Vert, code de sortie 0.** version-check + fmt-check + clippy `-D warnings` + test.
**103 tests** passent, 0 échec, 0 ignoré.

Working tree **propre**, branche `master` @ `2c7888f` (`Merge branch 'feat/v0.18.0'`).

## 2. Ce que la spec demande et que le code ne peut pas fournir

Trois vérifications faites **avant** d'écrire le plan, en lisant le code et les deux
dépôts voisins. Chacune a changé le périmètre.

### 2.1 Progression en octets — absente des deux libs

`design/tui/handoff.md` demande une cellule `62%` dans la colonne `rom`, un débit
`12.1 MiB/s` et une ligne de détail `rom 2.4/3.9 MiB`. Aucune des deux libs n'expose
d'avancement :

- `internetarchive` — `src/download.rs:88` : `res.copy_to(&mut file)`, aucun callback.
- `screenscraper` — `media_download(m).fetch(&dest)`, même forme.

Le pourcentage **par téléchargement en cours** est donc impossible sans toucher aux
dépôts voisins. En revanche le **volume** et le **débit moyen** restent calculables à la
granularité du fichier : le handler connaît la taille du fichier qu'il vient d'écrire.

→ Décision : handoff vers les deux libs, colonne `rom` en spinner pour v0.19, volume et
débit conservés à la granularité fichier.

### 2.2 « meilleur score 91 % » — n'existe pas côté ScreenScraper

`jeuRecherche.php` renvoie « une table de jeux (limité a 30 jeux) **classés par
probabilité** » (`../screenscraper/apiv2.html`). Le classement est réel, le pourcentage
non : il n'est renvoyé nulle part. Le champ `score` de l'API, seul candidat plausible,
est documenté « Note sur 20 de 0 a 20 » — c'est la note des utilisateurs sur le jeu,
pas une pertinence de recherche.

→ Décision : colonne **`rank`** (le rang réellement renvoyé par SS) au lieu d'un score
inventé, et la touche `a` (« accepter tous les ≥ 90 % ») disparaît. Accepter en masse
sur un rang serait exactement le dommage que P1.1 a fermé : un mauvais `description.xml`,
un `pkgver` bumpé pour lui, et un mauvais `ss_game_id` persisté.

### 2.3 `jeu_recherche` renvoie des `JeuInfo` complets — bonne surprise

`fetch_jeu_recherche` (`../screenscraper/src/api.rs:97`) renvoie `Vec<JeuInfo>`, et
`JeuInfo` porte `medias`, `editeur`, `genres`, `joueurs`, `noms` (avec régions).

Le handoff prévoyait « un appel `jeu_infos` par candidat, sinon ne remplir que le
candidat sélectionné ». C'est inutile : les pastilles médias par candidat et la ligne
`selection` du modal se remplissent avec ce qui est **déjà en mémoire**, sans une seule
requête supplémentaire. `ModalCandidate` est simplement trop pauvre aujourd'hui
(nom, id, année).

## 3. Contraintes de vérification

Pas de terminal de contrôle côté Claude : `/dev/tty` est inouvrable, la TUI ratatui ne
peut pas être lancée ni regardée d'ici. Seuls les **fonctions pures** et le mode
`--plain` sont vérifiables automatiquement. Tout le rendu part dans `manual_tests.md`
et demande une relecture visuelle du mainteneur — c'est le point le plus important de
ce lot, puisqu'il *est* du rendu.
