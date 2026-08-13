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

## M9 — Une ROM inconnue de ScreenScraper est identifiable à la main

**Pourquoi :** ajouté en cours de lot. Avant `screenscraper` v0.8.1, une recherche par nom
sans résultat faisait échouer `LookupSS` et la modale ne s'ouvrait jamais.

1. Un système contenant une ROM que SS ne connaît ni par sha1 ni par nom — un multicart
   pirate fait l'affaire (*Dragon Ball Party 4 In 1*, `nes`)
2. Attendre qu'elle arrive dans la vue « à identifier » (`m`), `enter`, `i`, saisir un
   game id

**Attendu :** la ROM est dans la file `m` et non dans la vue erreurs ; le paquet sort avec
son `description.xml` rempli.

---

## Résultats — 2026-08-13

**Faux départ à consigner.** Le premier run de validation (`nes`, 1992 ROMs, 21 min 32 s)
a tourné sur une **image antérieure au changement** : `just ci` réécrit
`target/debug/rompom` à chaque passage, et le processus lancé avant gardait son image. Le
symptôme n'était pas « `r` ne marche pas » mais **deux lignes d'aide absentes à la fois**
(`R retry them` au bilan, `r/R` dans la vue erreurs) — deux libellés manquants ensemble
désignent un binaire, pas une touche non branchée. À retenir : `rompom --version` ne
départage pas tant que le bump n'est pas posé ; le témoin est le pied de page, ou
`strings ./result/bin/rompom | grep -q 'retry them'`.

| test | verdict |
|---|---|
| M1 — `r` relance la ROM sélectionnée | ✅ conforme |
| M2 — `R` relance toutes les échouées | ✅ conforme |
| M3 — une relance qui rate revient au bilan | ✅ conforme (les 3 ROMs SS du run `nes`) |
| M4 — `r` sur une ligne qui n'a pas échoué | non déroulé |
| M5 — `r` pendant le run | non déroulé |
| M6 — Ctrl-C pendant un tour de relance | non déroulé |
| M7 — `q` après une relance | non déroulé |
| M8 — flush de l'état pendant un tour de relance | non déroulé |
| M9 — l'identification manuelle après v0.8.1 | ✅ conforme, sur *Dragon Ball Party 4 In 1* |

**Portée de la validation, telle qu'elle est.** M1–M3 ont été passés sur le commit
précédant la montée `screenscraper` v0.8.1 (`ad928c7`) ; ce commit ne touche que
`Cargo.toml`, `Cargo.lock` et le hash Nix — aucune ligne du chemin de relance — donc ils
restent acquis. M9 a été passé après.

M4 à M8 **n'ont pas été déroulés** et la release a été acceptée sans eux, sur décision du
mainteneur. Le trou qui compte est **M6** : c'est le seul scénario qui exerce le correctif
`cut_short` (Ctrl-C pendant un tour de relance ne doit pas écrire un `run.yml` où tout est
`Done`), et aucun test unitaire ne le couvre — la boucle de tours demande un vrai pool et
un vrai terminal. À dérouler à la première occasion.

## Trouvé en chemin, pas corrigé

**Un échec dur qui n'en était pas un.** Les 3 ROMs en erreur du run `nes` portaient toutes
`ScreenScraper sent a response rompom could not parse`. Diagnostic complet dans
`handoff_screenscraper_empty_search.md` : `jeuRecherche` dit « rien trouvé » par
`jeux: [{}]`, ce qui faisait échouer tout l'appel et rendait la modale d'identification
inatteignable. Corrigé en amont (`screenscraper` v0.8.1), câblé ici en `ad928c7`.

**`rompom --plain | head` sort en 101.** Rust ignore `SIGPIPE` par défaut, donc `println!`
échoue sur un tube fermé et panique ; `install_panic_hook()` avale le message, il ne reste
que le code de sortie. Sans rapport avec ce lot, et invisible sans tube — mais le mode
`--plain` existe précisément pour être redirigé, donc ça mérite une ligne dans `TODO.md`
un jour.
