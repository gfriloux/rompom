# Tests manuels — v0.17.0

Enrichi au fil du développement, exécuté en validation avant de proposer la release.
Les cas marqués **(auto)** sont couverts par `cargo test` et ne sont rappelés ici que
pour la traçabilité.

## Phase 1 — bump screenscraper v0.7.0

- [x] `nix develop --command just ci` → exit 0, 34 tests. *(2026-08-10)*
- [x] `nix build` → succès, binaire musl produit. C'est la seule porte qui valide
      `cargoLock.outputHashes` ; `just ci` passe même avec un hash faux — vérifié en
      posant volontairement un hash bidon, qui n'a fait échouer que `nix build`.
- [x] `cargo build` n'affiche plus `unused manifest key: target.x86_64-unknown-linux-gnu`.

## Phase 2 — `StepError`

- [x] **(auto)** `Fatal` → `Failed` sans réessai ; `Transient` → réessaie jusqu'à
      `max_retries` ; `Interrupted` → step `Pending`, aucun `wait_for` décrémenté.
- [ ] Run complet sur un petit système (< 10 ROMs) : aucune différence de comportement
      observable par rapport à v0.16.0 — le refactor est mécanique.
- [ ] Ctrl-C en plein run, puis relance et réponse « oui » au resume : le run reprend
      comme avant.

## Phase 3 — P1.1

- [x] **(auto)** table `ApiFailure → StepError`, variante par variante — les 13, plus un
      test que le message ne peut pas porter de credentials.
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

- [x] **(auto)** troncature de la cause : cas court, cas long, panneau à 0/1/2 colonnes,
      espace flottant, retour à la ligne. Plus deux tests sur la restauration d'un échec.
- [ ] Provoquer un échec (réseau coupé sur une ROM) : le panneau Completed affiche
      `✗ Nom du jeu — cause`.
- [ ] Cause très longue : la ligne ne déborde pas, ne casse pas la mise en page,
      et le terminal réduit à ~60 colonnes reste lisible.
- [ ] Le bilan de fin liste les ROMs échouées avec leur cause **complète**.
- [ ] Reprise d'un run interrompu contenant une ROM déjà `Failed` : la cause restaurée
      depuis `run.yml` s'affiche aussi (chemin `restore_bar_for_resumed_rom`).

## Phase 5 — P1.3

- [x] `~/.config/rompom.yml` absent → message nommant le chemin attendu. *Exécuté* avec
      `XDG_CONFIG_HOME=/nonexistent` : « cannot read the configuration file
      /nonexistent/rompom.yml: No such file or directory (os error 2) ».
- [x] YAML volontairement cassé → *exécuté* : « invalid configuration in /…/rompom.yml:
      screenscraper.dev: missing field `password` at line 3 column 5 ».
- [ ] Fichier présent mais illisible (`chmod 000`) → message distinct du précédent.

## Phase 6 — P1.7

- [ ] Système OpenBOR (id 214), au moins 2 ROMs, run complet : chaque paquet contient
      **son** `launcher`, aucun fichier `launcher` ne traîne dans le répertoire courant.
- [ ] Diff des deux launchers générés : ils diffèrent bien (nom du jeu).

## Phase 7 — P1.8

- [x] **(auto)** round-trip `SystemState` (mtime/size/ss_game_id/médias/extra discs),
      fichier absent vs illisible, et l'écriture qui ne laisse ni `.tmp` ni `.old`.
- [ ] `kill -9` en plein run (≠ Ctrl-C) : au run suivant, les ROMs déjà traitées avant le
      dernier flush ne sont **pas** re-packagées et leur `pkgver` n'est pas re-bumpé.
- [ ] `state.yml` corrompu à la main → avertissement explicite au démarrage, au lieu du
      silence actuel.
- [ ] `kill -9` pendant l'écriture de `run.yml` : le fichier restant est soit l'ancien
      intact, soit le nouveau complet — jamais un YAML tronqué.

## Validation finale du lot

- [x] `nix develop --command just ci` → exit 0, 59 tests. *(2026-08-10)*
- [x] `nix build` → exit 0.
- [x] `just audit` : 9 avertissements autorisés, **aucun nouveau** par rapport aux ignores
      justifiés de `.cargo/audit.toml`.
- [x] `just changelog-preview` : 11 entrées, lisibles telles quelles.
- [ ] Run complet sur un système réel de bout en bout, sans interruption.
