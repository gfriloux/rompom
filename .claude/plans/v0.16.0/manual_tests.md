# Tests manuels — v0.16.0

Ce que les tests unitaires ne peuvent pas couvrir : vrais appels réseau, rendu ratatui,
modale, `makepkg` sur Batocera. À exécuter **avant** de taguer la version.

Enrichir au fil du développement. Cocher au moment de la validation finale.

---

## 1. Échappement PKGBUILD (P0.1)

- [ ] **Jeu au nom simple, non-régression.** Packager un ROM déjà packagé avant le plan
      (ex. un jeu Megadrive courant). Comparer le `PKGBUILD` et le `description.xml`
      générés avec ceux d'avant. *Attendu :* identiques, et **`pkgver` non bumpé**.
- [ ] **Jeu au nom riche.** Un jeu contenant `&`, `'`, `!`, une apostrophe typographique
      ou un accent (ex. « Astérix & Obélix »). *Attendu :* `pkgdesc` reste lisible,
      `pkgname` est propre, `makepkg --printsrcinfo` ne bronche pas.
- [ ] **`makepkg` réel sur Batocera.** Construire et installer un paquet généré par la
      nouvelle version. *Attendu :* le jeu apparaît dans EmulationStation avec ses médias.

## 2. Traversée de chemin (P0.2)

- [ ] Après un run complet, vérifier qu'aucun fichier n'a été écrit hors du répertoire de
      sortie : `find <sortie>/.. -newer <marqueur>` ne remonte rien d'inattendu.

## 3. Panique et échec de step (P0.3, P0.4)

- [ ] **Panique provoquée.** Rendre un fichier ROM local illisible (`chmod 000`) pendant
      que `ComputeHashes` tourne. *Attendu :* la ROM passe en `✗` avec une cause, le run
      **continue et se termine** — il ne se fige pas.
- [ ] **Pas de faux succès.** Après un échec de téléchargement de ROM, inspecter
      `<system>.state.yml`. *Attendu :* aucune entrée pour cette ROM, donc elle est
      retentée au run suivant.
- [ ] **Comptage UI.** Le total du panneau Completed = succès + inchangés + erreurs, sans
      double comptage.

## 4. Interruption et resume (P0.5)

- [ ] **Ctrl-C en pleine phase de packaging.** Interrompre pendant que des ROMs sont en
      `preparing...`. Relancer, répondre « oui » au resume.
      *Attendu :* les `description.xml` restent corrects (non vides), `pkgver` n'est pas
      bumpé sans raison, le cache `ss_game_id` du state est conservé.
- [ ] **Ctrl-C pendant la modale d'identification.** *Attendu :* sortie propre, terminal
      restauré, `run.yml` écrit.
- [ ] **Double Ctrl-C.** *Attendu :* sortie immédiate.
- [ ] **Resume répondu « non ».** *Attendu :* `run.yml` supprimé, run neuf.

## 5. Sorties d'erreur (P0.6, P1.3)

- [ ] **Credentials ScreenScraper faux.** *Attendu :* message clair sur stderr, terminal
      rendu propre, code de sortie non nul. Pas de trace de panique, pas de terminal cassé.
- [ ] **Glob invalide** dans `~/.config/rompom.yml` (ex. `filter: ["["]`). *Attendu :*
      message nommant le motif fautif et le système.
- [ ] **Répertoire source inexistant** pour une source `folder`. *Attendu :* message
      nommant le chemin.
- [ ] **YAML cassé** dans la config. *Attendu :* le chemin du fichier **et** la
      ligne/colonne serde_yaml apparaissent.
- [ ] **Item Internet Archive inexistant.** *Attendu :* erreur lisible, pas de panique.

## 6. Erreur réseau vs jeu introuvable (P1.1)

- [ ] **Hors ligne.** Couper le réseau au milieu d'un run.
      *Attendu :* des retries, **aucune modale d'identification** — l'absence de réseau
      n'est pas « jeu inconnu ».
- [ ] **Jeu réellement inconnu de SS.** Un ROM au nom fantaisiste.
      *Attendu :* la modale s'ouvre bien, elle.
- [ ] **Quota SS dépassé** (si reproductible). *Attendu :* retry, pas de modale.

## 7. Affichage des échecs (P1.2)

- [ ] Panneau Completed : `✗ <rom> — <cause>`, cause tronquée proprement si longue.
- [ ] `Summary::print()` liste les ROMs en échec avec leur cause en fin de run.
- [ ] Terminal étroit (< 100 colonnes) : pas de débordement ni de retour à la ligne cassé.

## 8. Non-régression générale

- [ ] Run complet sur un système **multi-disc** (Saturn ou PSX) : `.m3u` correct, tous les
      disques en source du PKGBUILD.
- [ ] Deuxième run immédiat sur le même système : **tout en `=`** (inchangé), aucun
      `pkgver` bumpé, aucun téléchargement.
- [ ] Run avec `--debug` : le `<system>.debug.log` est cohérent avec ce que l'UI affiche.
