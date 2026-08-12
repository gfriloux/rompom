# Tests manuels — v0.21.0

Ce que `just ci` ne peut pas couvrir : le vrai réseau (ScreenScraper, Internet Archive),
le rendu ratatui, la modale, et `makepkg`. Enrichi au fil du développement, exécuté avant
de proposer la release.

## M1 — L'ordre des pastilles suit l'ordre du téléchargement

**Pourquoi :** D1/D2 changent l'ordre de parcours des médias. À l'écran, les pastilles
doivent maintenant se remplir de **gauche à droite** au lieu de sauter entre les colonnes
`screenshot` et `bezel`.

1. `rompom -s <system>` sur un système à quelques ROMs neuves
2. Regarder une ligne en cours de téléchargement des médias

**Attendu :** les pastilles passent au vert dans l'ordre des colonnes, sans trou qui se
remplit après coup (hors média absent, qui reste `○`).

## M2 — Le PKGBUILD reste consommable par `makepkg`

**Pourquoi :** l'ordre des `sources` change (D2). La correspondance `sources` ↔ `sha1sums`
est positionnelle.

1. Prendre un paquet généré avec les huit médias
2. `makepkg -g` (ou `makepkg --verifysource`) dans son répertoire

**Attendu :** aucune erreur de somme, tous les fichiers trouvés. Vérifier à l'œil que la
n-ième entrée de `sha1sums` correspond bien à la n-ième `source`.

## M3 — `description.xml` désigne des fichiers qui existent

**Pourquoi :** B4 change la façon dont les chemins sont nommés.

1. Un paquet complet dans son répertoire de sortie
2. Pour chaque chemin `./data/<romname>/<fichier>` de `description.xml`, vérifier que le
   fichier correspondant est bien présent à côté du PKGBUILD

**Attendu :** correspondance exacte, extension comprise.

## M4 — Ctrl-C pendant un backoff sort tout de suite

**Pourquoi :** A3. Avant, un worker endormi jusqu'à 16 s retardait d'autant la sortie.

1. Provoquer un retry : couper le réseau en plein téléchargement (ou pointer un dossier
   dont un fichier disparaît en cours de route)
2. Attendre de voir `retrying (n/N)` en jaune sur une ligne
3. Ctrl-C

**Attendu :** l'interface rend la main sans attendre la fin du backoff, `run.yml` est
écrit, le message de reprise s'affiche sur un terminal restauré.

## M5 — La reprise rejoue bien un retry abandonné

**Pourquoi :** D5 — `shutdown()` jette les tâches datées.

1. Reprendre le run de M4 : `rompom -s <system>`, répondre **oui**
2. Suivre la ROM qui était en backoff

**Attendu :** elle est retraitée depuis le début de son pipeline (resume tout-ou-rien),
et le run se termine normalement.

## M6 — La modale répond toujours

**Pourquoi :** A2 touche le sémaphore que `LookupSS` et `WaitModal` acquièrent.

1. Un système avec au moins deux ROMs non identifiables
2. Laisser la file « à identifier » se former (`m`), en traiter une, annuler l'autre (`s`)

**Attendu :** aucun blocage, la vue `m` se vide, le run va au bout. Puis Ctrl-C avec une
modale ouverte : sortie propre.

## M7 — Un second run ne réécrit rien

**Pourquoi :** filet global du lot — B3 et B5 touchent la décision « rien n'a changé ».

1. Relancer `rompom -s <system>` sur une bibliothèque déjà à jour

**Attendu :** toutes les lignes en gris `unchanged`, aucun `pkgver` bumpé (vérifier deux
ou trois PKGBUILD), `state.yml` inchangé dans son contenu utile.

## M9 — Le manuel de Castlevania III (Europe)

**Pourquoi :** D1 — c'est le cas signalé. Système 3 (NES), jeu 1278.

1. Scraper cette ROM, `--debug` activé
2. Vérifier que `manual.pdf` est bien posé dans le répertoire du paquet
3. Vérifier l'entrée `manual.pdf::` du PKGBUILD

**Attendu :** l'URL du PKGBUILD est `…/medias/3/1278/manuel(fr).pdf` et **pas** `(eu)` ;
le fichier fait 6 305 431 octets, sha1 `a9a9c42c590c7401c99d4a28e11125b16e24ac4b`.
Aucune ligne « public path 404 » dans le log de debug — le repli ne doit pas servir ici,
c'est bien l'URL qui est corrigée.

## M10 — Le repli API sert quand il doit servir

**Pourquoi :** E1. À défaut d'un vrai 404, on le provoque.

1. Prendre un paquet déjà scrapé, supprimer un média du répertoire
2. Éditer temporairement `media_url()` pour renvoyer un nom de fichier inexistant
   (ou tester sur un jeu dont un asset 404 réellement, s'il en reste après D1)
3. Relancer

**Attendu :** le média est quand même récupéré, la pastille passe au vert, et
`<system>.debug.log` porte `public path 404 → fetched through the API`. **Vérifier qu'aucun
mot de passe n'apparaît** dans ce log, dans `<system>.errors.log`, ni dans le bilan.

## M8 — `--plain` et `--debug`

1. `rompom -s <system> --plain --debug` sur quelques ROMs
2. Lire `<system>.debug.log`

**Attendu :** une ligne par média dans le bloc `[BuildPackage]`, dans l'ordre de l'écran,
et les lignes `rom_unchanged` toujours présentes pour les deux origines (folder → préfixe
`[ComputeHashes]`, IA → `[LookupSS]`).

---

## Résultats

| test | date | résultat |
|---|---|---|
| M1 | 2026-08-12 | OK |
| M2 | 2026-08-12 | OK |
| M3 | 2026-08-12 | OK |
| M4 | 2026-08-12 | OK |
| M5 | 2026-08-12 | OK |
| M6 | 2026-08-12 | OK |
| M7 | 2026-08-12 | OK |
| M8 | 2026-08-12 | OK |
| M9 | 2026-08-12 | **OK** — Castlevania III (Europe), le manuel arrive |
| M10 | — | **non joué** — demande de casser `media_url()` exprès. Le repli n'a pas lieu de se déclencher maintenant que les URLs sont justes ; à rejouer si un 404 réapparaît en conditions réelles. |
