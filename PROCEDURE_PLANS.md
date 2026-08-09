# PROCEDURE_PLANS.md — Procédure de travail sur rompom

> Ce document définit le processus à suivre **systématiquement** avant tout travail sur le
> code. Chaque changement est décomposé en étapes atomiques, testables et commitables
> isolément.
>
> **Règle fondamentale : on ne code pas sans plan validé.**
> Et : lis [`CLAUDE.md`](CLAUDE.md) (architecture, pipeline, invariants) avant tout changement.

---

## 1. Créer le plan avant tout

Dès qu'une version ou une feature est évoquée, créer :

```
.claude/plans/v{X.Y.Z}/
  plan.md           ← contexte, périmètre, phases, décisions, fichiers touchés
  manual_tests.md   ← tests manuels (enrichis au fil du dev, exécutés en validation)
  phase0_results.md ← état réel du dépôt avant de coder (cf. §2)
```

Les plans vivent **dans `.claude/plans/`**, jamais à la racine. Un plan obsolète est
**supprimé**, pas dupliqué en `_v2`/`_v3`.

Les documents de cadrage déjà présents à la racine (`TODO.md`, `PLAN_DOCUMENTATION.md`,
`PLAN_SS_ROM_CONTRIBUTION.md`, `PLAN_STATE_MACHINE.md`) sont la **roadmap**, pas des plans
d'exécution : un plan de version en dérive et les référence.

### Contenu minimal de `plan.md`

- **Contexte** : d'où on part, pourquoi.
- **Objectif** : ce qu'on veut atteindre.
- **Périmètre** : in scope / out of scope explicites.
- **Étage(s) concerné(s)** : `conf` | `collect` | `pipeline` | `package` | `ui` | `nix` | `doc`.
- **État du working tree** : ce qui est déjà là, ce qui doit disparaître.
- **Phases / étapes atomiques ordonnées** : chacune avec vérification + message de commit.
- **Décisions techniques** : choix et justifications.

---

## 2. Phase 0 — Audit obligatoire

**Avant de toucher au code**, vérifier l'état réel — ne jamais supposer le dépôt propre :

```bash
nix develop --command just ci
```

Consigner le résultat dans `.claude/plans/v{X.Y.Z}/phase0_results.md`.

---

## 3. Politique git — hybride

- Claude travaille sur une **branche dédiée** (`feat/…`, `fix/…`, `chore/…`, `refactor/…`,
  `docs/…`), jamais directement sur `master`.
- Claude **commite atomiquement** : un changement logique = un commit, en
  [Conventional Commits](#convention-de-commit). Chaque commit passe les portes seul.
- Claude ne fait **jamais** `merge`, `push` ni `tag`. L'utilisateur relit, merge sur
  `master` et push.
- **Un plan se termine toujours par un merge sur `master`.** À la clôture (portes vertes),
  l'utilisateur merge la branche du plan, **avant** de démarrer le plan suivant. On ne
  laisse pas une branche terminée non mergée : chaque plan part d'un `master` à jour.

### Convention de commit

```
type(scope): message court à l'impératif
```

- **type** : `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `chore`, `ci`, `build`.
- **scope** : l'étage réellement touché.

| scope | couvre |
|---|---|
| `conf` | `src/conf/` — chargement, `--update-config` |
| `collect` | collecte IA/folder, groupement multi-disc (`main.rs`) |
| `pipeline` | `src/worker/`, `src/rom/`, `src/queue.rs` — DAG, steps, handlers |
| `package` | `src/package.rs`, `src/emulationstation.rs`, `assets/templates/` |
| `state` | `src/state.rs`, `worker/run_state.rs` — state.yml / run.yml |
| `ui` | `src/ui/`, `src/summary.rs` |
| `nix` | `flake.nix`, `packages/`, `modules/`, `shells/`, `checks/` |
| `ci` | `.github/workflows/`, `Justfile`, pre-commit |
| `deps` | mises à jour de dépendances (utilisé par Renovate) |
| `release` | bump de version (**exclu du changelog**) |

**Le scope `all` est proscrit.** Il ne dit rien et pollue le changelog.

### Le message de commit EST l'entrée de changelog

Depuis l'adoption de git-cliff, `CHANGELOG.md` est **généré depuis les sujets de commit**
(cf. §6). Un sujet vague produit une entrée de changelog vague — il n'y a plus de passe
d'écriture manuelle pour rattraper.

```
✗ feat(all): add support for multi-disk systems
✓ feat(collect): group multi-disc files into a single package
✓ feat(package): build an .m3u playlist for multi-disc games
✓ fix(package): use the real media extension in PKGBUILD sources

✗ feat(all): update Cargo.lock          → chore(deps): update Cargo.lock
✗ feat(all): bump version to v0.14.1    → chore(release): v0.14.1
```

Un gros changement se découpe en plusieurs commits, un par entrée de changelog souhaitée.
Une **rupture** se marque `type(scope)!:` avec un paragraphe `BREAKING CHANGE:` dans le
corps — git-cliff le remonte en tête d'entrée.

La doc se met à jour **dans le même commit** que le code qu'elle décrit. Un changement
structurel commité sans MAJ de `CLAUDE.md`/`README.md` rend la doc périmée — c'est un défaut.

---

## 4. Discipline de test

rompom n'a **aucun test** aujourd'hui ; c'est la dette P1.6 de `TODO.md`. La règle à partir
de maintenant : **tout nouveau code pur arrive avec son test**, et toute correction de bug
commence par un test qui échoue.

**On automatise** (fonctions pures, sans réseau ni terminal) :
`disc_indicator()`, `group_multi_disc()`, `search_name()`, `normalize_name()`,
`check_media_changes()`, `read_pkgver()`, `apply_game_path()`, round-trip `SystemState`,
`apply_run_state()` (invariant anti-underflow), et le **snapshot XML** de
`generate_description_xml()`.

**On n'automatise pas** : les vrais appels ScreenScraper / Internet Archive, le rendu
ratatui, la modale d'identification, `makepkg` sur Batocera. Ces points partent dans
`manual_tests.md` du plan.

Un snapshot qui change par accident = **blocage dur**. Le régénérer est un acte
intentionnel, et le diff se relit.

---

## 5. Types de changement & recettes

| Type | Étage | Étapes (chacune = 1 commit) |
|---|---|---|
| Nouveau step de pipeline | `pipeline` | 1. step + handler (`feat(pipeline): …`) → 2. test → 3. MAJ du DAG dans `CLAUDE.md` |
| Nouveau template PKGBUILD | `package` | 1. template + sélection (`feat(package): …`) → 2. snapshot → 3. doc |
| Changement de `description.xml` | `package` | 1. snapshot mis à jour (`test(package): …`) → 2. impl → 3. doc |
| Changement d'UI / phase | `ui` | 1. impl (`feat(ui): …`) → 2. `manual_tests.md` → relecture visuelle |
| Correction de bug | étage concerné | 1. test de régression qui échoue (`test: reproduce …`) → 2. fix (`fix(scope): …`) |
| Refactor | étage concerné | 1. refactor sans changer de snapshot (`refactor(scope): …`). Si un snapshot bouge, ce n'était pas un refactor. |
| Changement de format d'état | `state` | migration explicite + note de rupture ; jamais de changement silencieux de `state.yml` |
| Doc seule | — | `docs: …` |

---

## 6. Release

**SemVer.** Le changelog est dérivé des Conventional Commits par **git-cliff**. Le tag est
posé par le **mainteneur** (politique git hybride) et déclenche la publication.

```bash
just release 0.16.0     # bump Cargo.toml + Cargo.lock + packages/rompom/default.nix
                        # puis régénère CHANGELOG.md avec --tag v0.16.0
```

Relire le diff du changelog, commiter (`chore(release): v0.16.0`), merger sur `master`, puis :

```bash
git tag -a v0.16.0 -m "v0.16.0" && git push origin v0.16.0
```

Le tag déclenche `.github/workflows/release.yml`, qui :

1. **vérifie que le tag correspond à la version de `Cargo.toml`** (sinon échec) ;
2. génère les notes avec `git-cliff --latest --strip header` — c'est **exactement** la
   section `CHANGELOG.md` de cette version ;
3. build le binaire statique musl via `nix build` et l'attache à la release.

`CHANGELOG.md` est **généré** : ne jamais l'éditer à la main, lancer `just changelog`.
Les entrées **v0.15.0 et antérieures** sont figées dans
[`CHANGELOG-legacy.md`](CHANGELOG-legacy.md) et ne sont jamais régénérées.

`just changelog-preview` montre ce que publierait la prochaine release.

---

## 7. Portes de qualité

Tout changement passe ces portes avant d'être considéré comme terminé. **Une seule
définition** : le `Justfile`. pre-commit et la CI l'appellent.

```bash
just version-check  # Cargo.toml == Cargo.lock == packages/rompom/default.nix
just fmt-check      # rustfmt --config tab_spaces=2
just lint           # clippy -D warnings, aucun warning toléré
just test           # tests unitaires
just ci             # les quatre d'affilée
```

En plus, hors boucle rapide : `just audit` (CVE) et `nix flake check`
(alejandra / deadnix / statix / rustfmt), tous deux lancés par la CI.

---

## 8. Gabarit de plan

```markdown
## Plan : [Titre]

**Type :** [pipeline | package | ui | conf | state | bug | refactor | doc]
**Objectif :** ...
**Pourquoi :** ...
**Étage(s) :** [conf | collect | pipeline | package | state | ui | nix | doc]

### Fichiers touchés
- [ ] `src/...`
- [ ] `assets/templates/...`
- [ ] `CLAUDE.md` / `README.md`

### Étapes atomiques
#### Étape 1 : [Titre]
**Description :** ...
**Vérification :** `just ci` (ou la cible pertinente)
**Commit :** `type(scope): message`

### Portes de qualité
- [ ] `just ci` passe
- [ ] Tests ajoutés pour le code pur touché
- [ ] Doc synchronisée (même commit)
- [ ] Commits atomiques, scope réel (jamais `all`), sujets de qualité changelog
- [ ] Branche dédiée, non mergée par Claude
```

---

## 9. Ce qui ne change pas entre les versions

- **Un `description.xml` par paquet ROM.** Le `gamelist.xml` est régénéré par hook
  post-install Batocera — rompom ne l'écrit jamais.
- **Vérification SHA1 systématique** avant de considérer un fichier comme présent.
- **L'état est incrémental** : `state.yml` fait foi pour décider ce qui a changé ; un
  `pkgver` ne se bump que si ROM, média ou `description.xml` a réellement changé.
- **Le pipeline est un DAG** de steps avec `wait_for`/`next` — pas de séquence codée en dur.
- **Les credentials ScreenScraper sont un secret** : jamais dans un log, un test, une
  fixture ou un message d'erreur.
- **Git hybride** : branche + commits atomiques par Claude ; merge/push/tag par l'utilisateur.
- **Nix** : toujours `nix develop --command …` pour les commandes non interactives.
- **`tmp/`** : scratch non commité (handoffs design, notes, sorties de travail).
- **`CHANGELOG.md` est généré**, `CHANGELOG-legacy.md` est figé.
- **Le `Justfile` est la seule définition des portes.** pre-commit et la CI l'appellent ;
  on n'ajoute jamais une vérification dans la CI sans l'ajouter au Justfile.

---

**Dernière mise à jour :** 2026-08-09
**Statut :** Actif
