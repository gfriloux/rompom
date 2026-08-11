# `design/tui/` — spec de la refonte TUI « turn 4 »

Ces deux fichiers viennent d'un handoff de design produit hors du dépôt. Ils décrivent
**P2.7** de [`TODO.md`](../../TODO.md), la refonte de `src/ui/` **livrée en v0.19.0**.

> Trois demandes de la spec n'ont pas été livrées, faute de données et non de temps :
> le `62 %` par téléchargement (ni `internetarchive` ni `screenscraper` ne rapporte la
> progression — handoffs dans `.claude/plans/v0.19.0/`), le score de pertinence `91 %`
> et la touche `a` qui en dépendait (`jeuRecherche` classe par probabilité et ne renvoie
> aucun pourcentage ; son champ `score` est une note sur 20), et la relance `r`/`R`
> (du pipeline, pas de l'UI — point P3). Le reste est conforme, largeurs et raccourcis
> compris, avec les libellés traduits en anglais pour rester cohérents avec le reste de
> l'outil.

| fichier | quoi |
|---|---|
| `handoff.md` | la spec : ce que la refonte supprime (`PANELS`/`RomPhase`/`render_active()`), la grille qui les remplace, les vues filtrées, le modal réaligné, la vue repliée < 100 colonnes |
| `mockups.dc.html` | les maquettes, à ouvrir dans un navigateur. Le HTML **simule** une grille monospace : chaque `<div>` est une ligne de terminal, chaque `<span class="c" style="width:Nch">` une colonne de N cellules. Ce n'est pas du code à reprendre — la cible est ratatui. |

Ils vivaient dans `tmp/`, qui est du scratch non versionné par convention
(`PROCEDURE_PLANS.md` §9) : un `rm -rf tmp` et la spec était perdue.
