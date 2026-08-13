# Tests manuels — v0.22.0

Ce que `just ci` ne peut pas couvrir : le rendu ratatui, les touches, et un vrai échec de
step. Enrichi au fil du développement, exécuté avant de proposer la release.

**Comment provoquer un échec à volonté**, sans dépendre du réseau : source `folder`, et
`chmod 000` sur un fichier ROM avant le run. `ComputeHashes` échoue à l'ouverture
(`StepError` sur `io::Error`), la ROM part en erreur, et un `chmod 644` avant d'appuyer sur
`r` fait que la relance doit réussir. Un échec de téléchargement se provoque en coupant le
réseau pendant un run sur source IA.

## M1 — `r` relance la ROM sélectionnée et elle réussit

1. Dossier de 5 ROMs, une en `chmod 000`
2. `rompom -s <system>` → attendre le bilan, `1 failed`
3. `chmod 644` sur la ROM fautive (autre terminal)
4. `e` pour la vue erreurs, la sélectionner, `r`

**Attendu :** le bandeau redevient la barre de progression, la ligne repasse en
`queued` puis se déroule (`id`/`pkg`/`rom`, pastilles), le bilan revient avec
`0 failed`. Le `PKGBUILD` et le `description.xml` sont là, `pkgver = 1` (pas 2 : rien n'a
été écrit au premier tour).

## M2 — `R` relance toutes les échouées

1. Trois ROMs en `chmod 000`, run jusqu'au bilan
2. Les remettre lisibles, puis `R` depuis l'écran de bilan

**Attendu :** les trois repartent ensemble, le compteur `✗` retombe à 0.

## M3 — Une relance qui rate revient au bilan

1. Même chose, mais **sans** remettre les droits avant d'appuyer sur `r`

**Attendu :** la ROM refait un tour, échoue à nouveau, la cause est la même dans la vue
erreurs, `attempts` repart de 1, et le bilan se réaffiche. Pas de blocage : la queue se
referme normalement quand la ROM re-armée atteint sa feuille.

## M4 — `r` sur une ligne qui n'a pas échoué

1. Bilan, sélectionner une ROM verte, `r`

**Attendu :** un `notice` le dit, rien ne se relance. Pareil pour `R` sans aucun échec.

## M5 — `r` pendant le run

1. En cours de run, `e`, sélectionner une erreur, `r`

**Attendu :** le `notice` « retry is available once the run has finished ». Le run continue
sans être perturbé.

## M6 — Ctrl-C pendant un tour de relance

1. Relancer plusieurs ROMs (`R`), puis Ctrl-C pendant qu'elles tournent

**Attendu :** sortie propre, `<system>.run.yml` écrit avec le message habituel, terminal
restauré. Relancer `rompom -s <system>` propose bien le resume.

## M7 — `q` après une relance

1. Après un tour de relance terminé, `q`

**Attendu :** l'interface rend la main, `state.yml` contient l'entrée de la ROM relancée
(`ss_game_id`, sha1 ROM et médias), et `<system>.run.yml` a bien été supprimé.

## M8 — L'état est écrit pendant un long tour de relance

**Pourquoi :** le thread de flush sort de la boucle de tours ; il doit couvrir la relance
comme le run.

1. Relancer une ROM lourde, attendre > 30 s, regarder la date de `state.yml`

**Attendu :** le fichier est réécrit pendant le tour, pas seulement à la fin.

---

## Résultats

*(à remplir à l'exécution)*
