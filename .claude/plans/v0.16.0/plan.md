# Plan : v0.16.0 — Chemins d'échec de première classe

> **Statut : clos le 2026-08-09.** Les étapes 0 à 7 sont livrées — **P0 complet**.
> La phase 5 (P1.1–P1.3), annoncée séparable dès l'écriture du plan, a été **sortie du
> périmètre** : le lot P0 forme un ensemble cohérent et publiable, et P1.1 s'est révélé
> bloqué par la lib `screenscraper`, qui jette le statut HTTP et rend un `404`
> indistinguable d'un `430` (détail dans `TODO.md`). Reportée en v0.17.

**Type :** bug / sécurité (P0 complet + P1.1–P1.3)
**Étages :** `package`, `pipeline`, `state`, `collect`, `conf`, `ui`
**Phase 0 :** [`phase0_results.md`](phase0_results.md) — vert, `509d935`

## Contexte

La revue du 2026-08-08 (`TODO.md`) conclut : *le chemin nominal est soigné, mais les
chemins d'échec ne sont pas traités comme des chemins de première classe.* Les six P0 sont
tous des variantes de ça — panique, step `Failed`, resume partiel, données réseau hostiles.

Le `TODO.md` prévoyait ce lot en `v0.15.1` (patch). Il devient **`v0.16.0`** parce que
l'outillage (Justfile, CI, changelog généré, montée quick-xml, premiers tests) a déjà
atterri sur `master` sans release : le prochain tag contient des ajouts, pas seulement des
correctifs. Le séquencement de `TODO.md` sera mis à jour en fin de plan.

## Objectif

Aucune donnée venue de ScreenScraper ou d'Internet Archive ne peut plus provoquer
d'exécution de code, d'écriture hors du répertoire de sortie, de blocage global, ni de
persistance d'un état faux. Toute sortie en erreur est lisible.

## Périmètre

**In scope** — P0.1 à P0.6, P1.1, P1.2, P1.3.

**Out of scope** — P1.4 (pinning Cargo.toml), P1.6 au-delà des tests écrits ici, P1.7,
P1.8, tout P2 et P3. La refonte TUI (P2.7) reste en v0.18 : P1.2 lui prépare le terrain
mais ne l'entame pas.

## Décisions techniques

1. **Liste blanche, pas liste noire.** `normalize_name()` énumère aujourd'hui les
   caractères à retirer et en oublie (`"`, backtick, `\`, `\n`). On inverse : on garde
   `[A-Za-z0-9._-]`, tout le reste tombe. Un nom de jeu ne peut alors plus rien injecter,
   quelle que soit la créativité de ScreenScraper.

   **Arbitrage validé par l'utilisateur le 2026-08-09 :** on accepte que des `pkgname`
   existants changent, donc qu'un `pkgver` soit bumpé et qu'un paquet soit renommé au
   prochain run. *« Tant pis si cela renomme des packages — ils n'étaient pas conformes. »*
   Le risque correspondant est donc clos, pas à surveiller.

2. **Échappement au point d'injection, pas à la source.** `pkgdesc` doit rester lisible
   (« Sonic & Knuckles »), donc on ne le normalise pas : on l'échappe pour le shell au
   moment de le rendre dans le template. Une fonction unique, appliquée à tout champ
   SS/IA, plutôt qu'un filtre par champ qu'on oubliera d'étendre.

3. **`catch_unwind` avec `AssertUnwindSafe`.** Les handlers prennent `&Arc<Mutex<Rom>>` ;
   après une panique le mutex est empoisonné. On convertit la panique en `Failed(msg)` et
   on laisse P0.4 gérer la suite. Un `Rom` dont l'état est douteux ne doit surtout pas
   voir son `SaveState` s'exécuter — d'où l'ordre P0.3 puis P0.4.

4. **Échec définitif = successeurs `Skipped`, pas `Pending`.** Le compteur `remaining`
   doit quand même tomber à zéro, sinon la queue ne s'arrête jamais. On propage donc un
   `Skipped` le long du DAG au lieu de simplement ne pas dispatcher.

   **Correction apportée à l'étape 3 :** cette conception ne suffit pas. `remaining`
   n'est décrémenté que **dans `handle_save_state`** (`save_state.rs:96`), et le chemin
   rapide `Skipped` de `execute_step` retourne **sans exécuter le handler**. Propager un
   `Skipped` jusqu'à `SaveState` laisserait donc `remaining` bloqué — exactement le
   blocage que P0.3 corrige. L'étape 4 doit **déplacer la décrémentation dans
   `execute_step`**, sur la fin de pipeline, pour qu'elle ait lieu une fois par ROM que
   la fin soit `Done`, `Skipped` ou `Failed`.

7. **`panic = "abort"` doit disparaître du profil release Nix.** Il était posé pour la
   taille du binaire ; il rend `catch_unwind` inopérant — le process avorte. Le correctif
   P0.3 aurait été du code mort dans le seul build que les utilisateurs lancent.

8. **Une panique ne se retente pas.** Un dépassement d'index ou un `unwrap()` sur `None`
   retombera à l'identique : la logique de retry ne ferait que dépenser le backoff pour
   arriver au même échec. Les paniques vont directement en `Failed`.

5. **Au resume, on re-dérive plutôt qu'on ne devine.** `run.yml` ne stocke pas `jeu`/
   `medias`. Plutôt que de les sérialiser (format d'état supplémentaire à maintenir), on
   remet `LookupSS` à `Pending` dès que `BuildPackage` ou `SaveState` est `Pending`. Coût :
   un appel SS de plus au resume, servi par le cache `ss_game_id`.

6. **Sortir du TUI avant d'écrire sur stderr.** Un `eprintln!` en mode raw laisse le
   terminal inutilisable. Toute sortie en erreur passe par une restauration explicite du
   terminal d'abord.

## Fichiers touchés

- [ ] `src/package.rs` — échappement, liste blanche, validation sha1
- [ ] `src/worker/helpers.rs` — `media_filename()`
- [ ] `src/worker/mod.rs` — `execute_step`, `do_dispatch`
- [ ] `src/worker/run_state.rs` — `apply_run_state`
- [ ] `src/worker/handlers/discovery.rs` — erreur SS ≠ jeu introuvable
- [ ] `src/main.rs` — `unwrap()` de la collecte
- [ ] `src/emulationstation.rs` — date SS malformée
- [ ] `src/conf/mod.rs` — messages d'erreur
- [ ] `src/ui/mod.rs`, `src/summary.rs` — cause d'échec affichée
- [ ] `CLAUDE.md` — mêmes commits que le code décrit
- [ ] `TODO.md` — cases cochées, séquencement rafraîchi

---

## Étapes atomiques

### Phase préalable

#### Étape 0 : PR Renovate de sécurité
**Description :** merger la PR #19 (openssl 0.10.80, [SECURITY]) et la #20 (chrono).
**Vérification :** `just ci` + `just audit`
**Commit :** *(Renovate — merge, pas de commit de notre part)*

### Phase 1 — Injection et traversée de chemin (P0.1, P0.2)

#### Étape 1 : échappement des champs injectés
**Description :** tests d'abord (observés rouges) montrant qu'un nom SS hostile
(`$(id)`, backtick, `"`, `\n`) traverse `normalize_name()` et atteint le PKGBUILD, et
qu'un sha1 non hexadécimal est accepté. Puis le correctif : fonction d'échappement shell
unique appliquée à `pkgdesc`, `romname`, `pkgname` ; `normalize_name()` en liste blanche
`[A-Za-z0-9._-]` ; liste blanche alphanumérique pour `format`/`region` des médias ;
`sha1` validé contre `^[0-9a-f]{40}$`.
**Vérification :** `just ci`
**Commit :** `fix(package): escape every ScreenScraper field injected into a PKGBUILD`

#### Étape 2 : traversée de chemin sur les médias
**Description :** test montrant que `media_filename("image", "png/../../x")` s'échappe du
répertoire de sortie, puis rejet de `/`, `\` et `..`.
**Vérification :** `just ci`
**Commit :** `fix(pipeline): reject path separators in media filenames`

### Phase 2 — Le DAG survit à l'échec (P0.3, P0.4)

#### Étape 3 : une panique de handler devient un échec
**Description :** `catch_unwind(AssertUnwindSafe(...))` autour du dispatch des handlers
dans `execute_step` ; la panique devient `Failed` au lieu de tuer le worker et de laisser
`remaining` bloqué.
**Vérification :** `just ci` + test d'un handler qui panique.
**Commit :** `fix(pipeline): turn a handler panic into a failed step instead of a deadlock`

#### Étape 4 : un échec définitif n'empoisonne plus l'état
**Description :** après `Failed`, marquer les successeurs `Skipped` au lieu de les
dispatcher — `SaveState` ne persiste plus le sha1 d'une ROM jamais téléchargée, et l'UI
ne compte plus 1 erreur *et* 1 succès pour la même ROM.
**Vérification :** `just ci` + test sur la propagation dans le DAG.
**Commit :** `fix(pipeline): stop dispatching successors after a definitive failure`

### Phase 3 — Le resume ne corrompt plus les paquets (P0.5)

#### Étape 5 : re-dériver `jeu`/`medias` au resume
**Description :** tests d'abord — l'invariant anti-underflow de `apply_run_state`, puis le
cas de corruption : une interruption entre `LookupSS` (Done) et `BuildPackage` (Pending)
produit un `description.xml` vide au resume. Puis le correctif : si `BuildPackage` ou
`SaveState` est `Pending`, remettre `LookupSS` à `Pending`.
**Vérification :** `just ci` + test manuel de resume (cf. `manual_tests.md`)
**Commit :** `fix(state): re-run the ScreenScraper lookup when resuming an unfinished package`

### Phase 4 — Sorties d'erreur lisibles (P0.6)

#### Étape 6 : la collecte ne panique plus
**Description :** remplacer les `unwrap()` de collecte (`Metadata::get`, `Pattern::new`,
`read_dir`, `ScreenScraper::new`) par des erreurs remontées ; sortir du TUI avant
d'écrire sur stderr. Les `lock().unwrap()` restent.
**Vérification :** `just ci` + essai avec credentials faux et glob invalide.
**Commit :** `fix(collect): report collection failures instead of panicking under the TUI`

#### Étape 7 : date ScreenScraper malformée
**Description :** `emulationstation.rs:73` — `parse_from_str(...).unwrap()` sur une date
SS invalide tue le worker. Repli sur la date epoch déjà prévue plus haut.
**Vérification :** `just ci` + test avec date `0000-00-00` et date absurde.
**Commit :** `fix(package): fall back to the epoch on a malformed ScreenScraper date`

### Phase 5 — Confiance (P1.1, P1.2, P1.3) — séparable

> Cette phase peut être décalée en v0.17 sans rien casser des précédentes. Elle est ici
> parce que P1.1 touche `discovery.rs`, déjà ouvert par la phase 3.

#### Étape 8 : erreur réseau ≠ jeu introuvable
**Description :** `discovery.rs:250,255` — distinguer `Err` (retry, le retry de `LookupSS`
est aujourd'hui du code mort) de `Ok(None)` (modale d'identification). Supprime les
modales injustifiées sur timeout, 500 ou quota dépassé.
**Vérification :** `just ci` + essai hors ligne (cf. `manual_tests.md`)
**Commit :** `fix(pipeline): tell a ScreenScraper network error from a missing game`

#### Étape 9 : afficher la cause des échecs
**Description :** propager le message de `StepStatus::Failed(msg)` jusqu'à
`finish_error()` ; panneau Completed `✗ rom — cause` ; liste des échecs dans
`Summary::print()`.
**Vérification :** `just ci` + relecture visuelle
**Commit :** `feat(ui): show the failure cause for each failed ROM`

#### Étape 10 : messages d'erreur de configuration
**Description :** `#[snafu(display)]` sur `ReadConfiguration`/`ParseConfiguration` pour
conserver le chemin et l'erreur serde_yaml (ligne/colonne).
**Vérification :** `just ci` + config volontairement cassée
**Commit :** `fix(conf): keep the file path and the YAML error in configuration failures`

### Clôture

#### Étape 11 : documentation et roadmap
**Description :** `CLAUDE.md` (comportement du DAG en échec, resume, échappement),
`TODO.md` (cocher P0.1–P0.6, P1.1–P1.3, rafraîchir le séquencement).
**Commit :** `docs: record the failure-path behaviour and tick off the P0 lot`

#### Étape 12 : release
**Description :** `just release 0.16.0`, relire le changelog généré, puis l'utilisateur
tague.
**Commit :** `chore(release): v0.16.0`

---

## Portes de qualité

- [ ] `just ci` passe à **chaque** étape, pas seulement à la fin
- [ ] Chaque correction P0 précédée d'un test qui échoue
- [ ] `just audit` toujours à 0 vulnérabilité
- [ ] Doc synchronisée dans le même commit que le code
- [ ] Commits atomiques, scope réel (jamais `all`), sujets de qualité changelog
- [ ] Branche dédiée, non mergée par Claude
- [ ] `manual_tests.md` exécuté avant la release

## Risques

| Risque | Parade |
|---|---|
| L'échappement casse des PKGBUILD qui marchaient | Snapshot PKGBUILD avant/après sur un jeu au nom simple, en plus des tests hostiles |
| ~~La liste blanche renomme des `pkgname` existants~~ | **Arbitré le 2026-08-09 : accepté.** Les noms non conformes doivent changer. À signaler dans le changelog. |
| `catch_unwind` masque un vrai bug | Le message de panique part dans `Failed(msg)`, donc visible via l'étape 11 |
| Phase 5 fait déborder la version | Elle est séparable : la décaler en v0.17 ne casse rien |
