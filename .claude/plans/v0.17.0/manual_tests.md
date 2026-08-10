# Tests manuels — v0.17.0

Enrichi au fil du développement, exécuté en validation avant de proposer la release.
Les cas marqués **(auto)** sont couverts par `cargo test` et ne sont rappelés ici que
pour la traçabilité.

## Phase 1 — bump screenscraper v0.7.0

- [ ] `nix develop --command just ci` → exit 0.
- [ ] `nix build` → succès. C'est la seule porte qui valide `cargoLock.outputHashes` ;
      `just ci` passe même avec un hash faux.
- [ ] `cargo build` n'affiche plus `unused manifest key: target.x86_64-unknown-linux-gnu`.

## Phase 2 — `StepError`

- [ ] **(auto)** `Fatal` → `Failed` sans réessai ; `Transient` → réessaie jusqu'à
      `max_retries` ; `Interrupted` → step `Pending`, aucun `wait_for` décrémenté.
- [ ] Run complet sur un petit système (< 10 ROMs) : aucune différence de comportement
      observable par rapport à v0.16.0 — le refactor est mécanique.
- [ ] Ctrl-C en plein run, puis relance et réponse « oui » au resume : le run reprend
      comme avant.

## Phase 3 — P1.1

- [ ] **(auto)** table `ApiFailure → StepError`, variante par variante.
- [ ] **ROM réellement inconnue de SS** (fichier renommé en `zzz_unknown_xyz.zip`) :
      la modale s'ouvre, comme aujourd'hui. *C'est le seul cas qui doit encore l'ouvrir.*
- [ ] **Réseau coupé en plein run** (`nmcli` off, ou `/etc/hosts` renvoyant
      `screenscraper.fr` vers `127.0.0.1`) : **aucune modale**. Le step retente avec
      backoff, puis la ROM échoue avec une cause lisible.
- [ ] **Quota journalier dépassé (430)** — difficile à provoquer volontairement ; à
      guetter sur un gros run. Attendu : échec immédiat, sans les 3 tentatives.
- [ ] **Identifiants dev erronés (403)** : modifier `~/.config/rompom.yml` avec un
      `dev.password` faux → message clair au démarrage, **et pas de mot de passe dans
      la sortie** (vérifier aussi `<system>.debug.log`).
- [ ] **Modale, ID inexistant** (saisir `999999999`) : message inline « jeu introuvable ».
- [ ] **Modale, réseau coupé pendant la saisie d'un ID** : message inline distinct,
      parlant de réseau — pas « jeu introuvable ».

## Phase 4 — P1.2

- [ ] **(auto)** `Summary` construit depuis un `AppState` porteur d'échecs ; troncature.
- [ ] Provoquer un échec (réseau coupé sur une ROM) : le panneau Completed affiche
      `✗ Nom du jeu — cause`.
- [ ] Cause très longue : la ligne ne déborde pas, ne casse pas la mise en page,
      et le terminal réduit à ~60 colonnes reste lisible.
- [ ] Le bilan de fin liste les ROMs échouées avec leur cause **complète**.
- [ ] Reprise d'un run interrompu contenant une ROM déjà `Failed` : la cause restaurée
      depuis `run.yml` s'affiche aussi (chemin `restore_bar_for_resumed_rom`).

## Phase 5 — P1.3

- [ ] `~/.config/rompom.yml` absent → message nommant le chemin attendu.
- [ ] YAML volontairement cassé (indentation fautive) → message avec le chemin **et**
      la ligne/colonne remontée par serde_yaml.
- [ ] Fichier présent mais illisible (`chmod 000`) → message distinct du précédent.

## Phase 6 — P1.7

- [ ] Système OpenBOR (id 214), au moins 2 ROMs, run complet : chaque paquet contient
      **son** `launcher`, aucun fichier `launcher` ne traîne dans le répertoire courant.
- [ ] Diff des deux launchers générés : ils diffèrent bien (nom du jeu).

## Phase 7 — P1.8

- [ ] **(auto)** round-trip `SystemState` ; le flush périodique ne perd pas d'entrée.
- [ ] `kill -9` en plein run (≠ Ctrl-C) : au run suivant, les ROMs déjà traitées avant le
      dernier flush ne sont **pas** re-packagées et leur `pkgver` n'est pas re-bumpé.
- [ ] `state.yml` corrompu à la main → avertissement explicite au démarrage, au lieu du
      silence actuel.
- [ ] `kill -9` pendant l'écriture de `run.yml` : le fichier restant est soit l'ancien
      intact, soit le nouveau complet — jamais un YAML tronqué.

## Validation finale du lot

- [ ] `nix develop --command just ci` → exit 0.
- [ ] `nix build` → exit 0.
- [ ] `just audit` : aucune advisory **nouvelle** par rapport aux ignores justifiés
      de `.cargo/audit.toml`.
- [ ] `just changelog-preview` : chaque commit du lot produit une entrée lisible telle
      quelle dans les notes de release.
- [ ] Run complet sur un système réel de bout en bout, sans interruption.
