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
| M1 | | |
| M2 | | |
| M3 | | |
| M4 | | |
| M5 | | |
| M6 | | |
| M7 | | |
| M8 | | |
