# `design/tui/` — spec de la refonte TUI « turn 4 »

Ces deux fichiers viennent d'un handoff de design produit hors du dépôt. Ils décrivent
**P2.7** de [`TODO.md`](../../TODO.md), la refonte de `src/ui/` prévue pour v0.19.

| fichier | quoi |
|---|---|
| `handoff.md` | la spec : ce que la refonte supprime (`PANELS`/`RomPhase`/`render_active()`), la grille qui les remplace, les vues filtrées, le modal réaligné, la vue repliée < 100 colonnes |
| `mockups.dc.html` | les maquettes, à ouvrir dans un navigateur. Le HTML **simule** une grille monospace : chaque `<div>` est une ligne de terminal, chaque `<span class="c" style="width:Nch">` une colonne de N cellules. Ce n'est pas du code à reprendre — la cible est ratatui. |

Ils vivaient dans `tmp/`, qui est du scratch non versionné par convention
(`PROCEDURE_PLANS.md` §9) : un `rm -rf tmp` et la spec était perdue.
