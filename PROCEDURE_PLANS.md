# PROCEDURE_PLANS.md — Procédure de travail sur rompom

> Ce document définit le processus à suivre **systématiquement** avant tout travail sur le
> code. Chaque changement est décomposé en étapes atomiques, testables et commitables
> isolément.
>
> **Règle fondamentale : on dimensionne le changement avant de le planifier** (§1), et un
> changement architectural ne se code pas sans plan validé.
> Et : lis [`CLAUDE.md`](CLAUDE.md) (architecture, pipeline, invariants) avant tout changement.

---

## 1. Dimensionner un changement, puis créer le plan

Tout changement ne mérite pas un dossier de plan. **Classer d'abord, et annoncer le
classement à voix haute** pour que le mainteneur puisse le renverser, puis suivre ce
chemin-là.

| Chemin | Ce que c'est | Artefact |
|---|---|---|
| **Trivial** | Aucune décision à présenter : une coquille, un lien mort, une reformulation qui ne change ni comportement ni interface. | Aucun, et pas d'accord préalable. On le fait et on dit ce qu'on a fait. |
| **Spike** | Une question de faisabilité — « l'API ScreenScraper renvoie-t-elle ce champ ? », « ce `.chd` multi-piste est-il lisible ? ». La sortie est une **réponse**, pas du code qu'on garde. | Aucun. Énoncer la question et la sonde en deux phrases, obtenir un feu vert, puis sonder de la façon la moins coûteuse que la justesse permette. Rapporter une recommandation ; ce qui a été construit est étiqueté jetable. |
| **Bounded** (borné) | Un changement bien cadré sur du code **déjà présent ici** : un drapeau CLI, une correction dans un handler, un champ de plus dans `description.xml`, un ajustement de template. | Pas de fichier de plan. Présenter un design court en conversation, obtenir un **oui explicite**, puis implémenter. |
| **Architectural** | Un nouveau step de pipeline, un changement de format d'état, une refonte de l'UI, un nouveau type de source, tout ce qui déplace une responsabilité entre étages ou change le DAG. | Un plan sous `.claude/plans/`, relu par le mainteneur **avant** implémentation. |

Les noms *Spike*, *Bounded* et *Architectural* sont ceux du plugin `superpowers` (§10),
volontairement laissés en anglais : un lecteur qui part de ce
document pour grep le plugin doit retomber dessus. *Trivial* est à nous — le plugin n'a pas
ce chemin, et une coquille n'a pas à passer par un accord préalable.

Trois règles encadrent la table.

**Borné se mesure au dépôt, pas à la familiarité.** Si le flux qu'on change n'est pas déjà
là à lire, le changement n'est pas borné.

**Le cliquet est à sens unique.** Dans le doute entre deux chemins, prendre le plus lourd.
Une complexité découverte en cours de route fait monter d'un cran : on s'arrête, on le dit,
on remonte. Rien ne redescend en cours de route, et on ne choisit jamais une étiquette pour
s'épargner du travail.

**Pas de spec séparée.** Pour un changement architectural, `plan.md` **est** la spec —
c'est pourquoi il porte Objectif, Architecture, Contraintes globales et Review Focus (§9).

### Créer le plan (chemin architectural)

Dès qu'un chantier architectural est décidé — une version qui en porte un, ou un chantier
de process — créer :

```
.claude/plans/v{X.Y.Z}/
  plan.md           ← contexte, périmètre, phases, décisions, fichiers touchés
  manual_tests.md   ← tests manuels (enrichis au fil du dev, exécutés en validation)
  phase0_results.md ← état réel du dépôt avant de coder (cf. §2)
  progress.md       ← journal de bord : ce qui s'est réellement passé (cf. §8)
```

Un chantier de **process** n'est pas une version : il vit sous
`.claude/plans/process/<nom>/`, avec les mêmes fichiers, moins `manual_tests.md` quand il
n'y a rien à vérifier à la main.

Les plans vivent **dans `.claude/plans/`**, jamais à la racine. Un plan obsolète est
**supprimé**, pas dupliqué en `_v2`/`_v3`.

`TODO.md`, à la racine, est la **roadmap** — pas un plan d'exécution : un plan de version
en dérive et le référence. C'est le seul document de cadrage à la racine ; les plans de
cadrage historiques (`PLAN_DOCUMENTATION.md`, `PLAN_SS_ROM_CONTRIBUTION.md`,
`PLAN_STATE_MACHINE.md`) ont été supprimés le 2026-08-10, une fois réalisés ou repliés
dans `TODO.md`.

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

## 3. Politique git

- Claude travaille sur une **branche dédiée** (`feat/…`, `fix/…`, `chore/…`, `refactor/…`,
  `docs/…`, `ci/…`), jamais directement sur `master`.
- Claude **commite atomiquement** : un changement logique = un commit, en
  [Conventional Commits](#convention-de-commit). Chaque commit passe les portes seul.
- Claude **mène le flux git complet** — `commit`, `merge`, `tag`, `push` — et clôt une
  branche en **présentant les options d'intégration** plutôt qu'en décidant seul, après la
  relecture de clôture de §8. Toutes les opérations interactives passent par la règle
  ci-dessous.
- **Un plan se termine toujours par un merge sur `master`.** À la clôture (portes vertes),
  la branche du plan est mergée **avant** de démarrer le plan suivant. On ne laisse pas une
  branche terminée non mergée : chaque plan part d'un `master` à jour.

### Git interactif — annoncer le lot, puis enchaîner

Les commits et les tags sont signés OpenPGP, et le remote est en SSH adossé à une YubiKey.
Les deux ne coûtent pas la même chose, et la règle en découle :

| Opération | Ce qui protège | Coût |
|---|---|---|
| `commit`, `tag` | clé ed25519 **sur disque**, protégée par phrase de passe (`commit.gpgsign` et `tag.gpgsign` à `true`) | **une** saisie, puis `gpg-agent` met en cache. Aucun TTL n'est configuré, donc les défauts gpg : 10 min d'inactivité, 2 h au maximum |
| `fetch`, `pull`, `push` | clé SSH **`ED25519-SK`** — une YubiKey | **un contact physique par connexion**, qu'aucun cache n'évite |

`pinentry-qt` est configuré : la boîte de dialogue s'ouvre sur le bureau du mainteneur et
non dans le terminal de Claude — ce qui rend l'opération possible ici, où `/dev/tty` est
inouvrable.

D'où deux cadences distinctes :

1. **Annoncer le lot de commits et attendre le feu vert avant le premier** — « 5 commits
   atomiques arrivent, tu es au clavier ? » — puis les enchaîner en s'appuyant sur le cache.
2. **Demander confirmation avant chaque `fetch`, `pull`, `push` et `tag`** : un contact par
   connexion pour les trois premiers, et un tag peut tomber hors de la fenêtre de cache.

**Si une saisie traîne ou qu'une signature échoue, on s'arrête et on le dit.** Jamais de
`--no-gpg-sign`, jamais de contournement d'une signature manquante : un commit non signé
dans cet historique se voit à `git log --show-signature` et ne se rattrape pas sans le
réécrire.

### Convention de commit

```
type(scope): message court à l'impératif
```

- **type** : `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `chore`, `ci`, `build`.
- **scope** : l'étage réellement touché.

| scope | couvre |
|---|---|
| `cli` | `main.rs` — arguments, drapeaux, usage, codes de sortie |
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
| `procedure` | `PROCEDURE_PLANS.md` — le processus lui-même, avec sa reprise dans `CLAUDE.md` |
| `plans` | `.claude/plans/` — plans, journaux, résultats de phase 0, handoffs |

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

La règle : **tout nouveau code pur arrive avec son test**, et toute correction de bug
commence par un test qui échoue. La dette P1.6 de `TODO.md` est soldée depuis v0.18.0 —
les fonctions pures listées ci-dessous sont toutes couvertes, ce qui n'affranchit de rien
pour la suite.

**On automatise** (fonctions pures, sans réseau ni terminal) : `disc_indicator()`,
`group_multi_disc()`, `search_name()`, `normalize_name()`, `check_media_changes()`,
`read_pkgver()`, `apply_game_path()`, `sha1_file()`/`md5_file()`/`crc32_file()` sur les
vecteurs publiés, round-trip `SystemState`, `apply_run_state()` (invariant
anti-underflow), `disposition()` (politique de retry), et le **snapshot XML** de
`generate_description_xml()`.

**On n'automatise pas** : les vrais appels ScreenScraper / Internet Archive, le rendu
ratatui, la modale d'identification, `makepkg` sur Batocera. Ces points partent dans
`manual_tests.md` du plan.

Un snapshot qui change par accident = **blocage dur**. Le régénérer est un acte
intentionnel, et le diff se relit.

### Ce que la loi des tests lie, et ce qu'elle ne lie pas

L'*Iron Law* du plugin `superpowers` — « pas de code de production sans un test qui échoue
d'abord » — **tient ici**, contrairement à ce qu'on pourrait supposer d'un outil dont
l'essentiel du travail passe par le réseau : `just test` porte une vraie suite unitaire, et
une fonction pure se teste en trente secondes.

**Elle lie le code pur.** Pas de fonction pure nouvelle sans son test écrit d'abord et
**regardé échouer**. Un test qu'on n'a jamais vu rouge ne prouve rien : il peut tester
l'implémentation plutôt que le comportement, ou passer pour une raison étrangère à ce qu'on
croyait vérifier.

**Elle ne lie pas** ce que la liste ci-dessus range en manuel : les appels ScreenScraper et
Internet Archive, les handlers qui en dépendent, le rendu ratatui, la modale, `makepkg`. Un
test écrit là pour satisfaire le rituel ne teste rien — il faudrait mocker au point de ne
plus vérifier que le mock. Ces points vont dans `manual_tests.md`, et leur absence de
couverture automatique **se déclare** dans le plan plutôt que de se deviner.

**Le test et le correctif partent dans le même commit** (§5). C'est l'entorse assumée à la
granularité du plugin, qui commiterait le test rouge séparément : §3 exige que chaque commit
passe les portes seul, et un commit rouge casse `git bisect`. Le message de commit porte
alors la preuve, en décrivant ce que le test produisait avant le correctif.

**Un refactor ne change aucun test.** S'il faut toucher un test pour qu'un refactor passe,
ce n'était pas un refactor — c'est un changement de comportement, et il se planifie comme
tel (§5).

---

## 5. Types de changement & recettes

| Type | Étage | Étapes (chacune = 1 commit) |
|---|---|---|
| Nouveau step de pipeline | `pipeline` | 1. step + handler **avec le test de ce qui est pur** (`feat(pipeline): …`) → 2. MAJ du DAG dans `CLAUDE.md` |
| Nouveau template PKGBUILD | `package` | 1. template + sélection **+ snapshot** (`feat(package): …`) → 2. doc |
| Changement de `description.xml` | `package` | 1. snapshot mis à jour **puis** impl, dans le **même** commit (`feat(package): …`) → 2. doc |
| Changement d'UI / phase | `ui` | 1. impl (`feat(ui): …`) → 2. `manual_tests.md` → relecture visuelle |
| Correction de bug | étage concerné | test de régression **+** fix dans le **même** commit (`fix(scope): …`) — cf. note ci-dessous |
| Refactor | étage concerné | 1. refactor sans changer de snapshot (`refactor(scope): …`). Si un snapshot bouge, ce n'était pas un refactor. |
| Changement de format d'état | `state` | migration explicite + note de rupture ; jamais de changement silencieux de `state.yml` |
| Doc seule | — | `docs: …`, ou `docs(procedure): …` quand c'est le processus qui change |

**L'ordre des recettes est logique, pas commit par commit.** Un test s'écrit avant le code
qu'il couvre et se regarde échouer (§4), mais il est **commité avec lui** : §3 exige que
chaque commit passe les portes seul, donc ni snapshot rouge ni implémentation sans son test
ne partent isolément. Une flèche de cette table sépare deux commits ; elle ne sépare jamais
un test de ce qu'il couvre.

### Test de régression : observé rouge, commité vert

On écrit toujours le test **avant** le correctif et on le **regarde échouer** — sans ça,
rien ne prouve qu'il teste quelque chose. Mais il est commité **avec** le correctif, pas
avant.

Raison : §3 exige que chaque commit passe les portes seul. Un commit intermédiaire rouge
casse `git bisect` et interdit de relire l'historique en confiance. Le message de commit
porte alors la preuve, en décrivant ce que le test produisait avant le fix.

C'est la seule entorse à « une étape = un commit » : ici l'étape *est* la paire
test + correctif.

---

## 6. Release

**SemVer.** Le changelog est dérivé des Conventional Commits par **git-cliff**. Le tag
déclenche la publication ; il est signé et poussé sous la règle de §3, donc **confirmation
demandée avant**, séparément du lot de commits.

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

## 8. Discipline d'exécution

**Quatre règles**, quel que soit le type de changement : trois se tiennent à chaque étape —
la cause, la preuve, et la ligne de journal écrite avec le commit — et une ne joue qu'à la
clôture de la branche. La cinquième, la loi des tests, vit en §4 —
c'est là qu'est la discipline de test, et la séparer d'elle n'aurait servi qu'à la répéter.
Toutes sont adaptées du plugin `superpowers` ; là où leur formulation diffère de la sienne,
**ce document l'emporte**.

### Cause avant correctif

**Pas de correctif avant que la cause soit comprise.** Un correctif qui fait disparaître le
symptôme sans explication est un échec, même quand l'erreur disparaît.

1. **Lire l'erreur en entier.** Le message porte souvent la réponse : un `ApiFailure`
   nomme son code HTTP, un `ChecksumMismatch` donne les deux sha1, un panic donne sa
   localisation. On ne saute pas au diagnostic depuis le nom du step.
2. **Reproduire au plus étroit.** Descendre de `just ci` au test unitaire nommé, ou d'un run
   complet à une seule ROM. Une défaillance qu'on déclenche en une commande est une
   défaillance sur laquelle on peut raisonner.
3. **Regarder ce qui a changé.** `git diff`, les derniers commits, une dépendance bumpée,
   un tag de lib déplacé. Une casse apparue après une mise à jour de `screenscraper` ou
   d'`internetarchive` est un changement amont, pas un bug d'ici.
4. **Comparer à ce qui marche.** Un autre handler qui fait la même chose, un autre template,
   la même ROM sur un autre système. Lister **toutes** les différences : « ça ne peut pas
   compter » est la phrase par laquelle on saute la cause.
5. **Une hypothèse, un changement minimal.** L'énoncer — « je pense que `media_url()` dérive
   le nom de `region` au lieu du paramètre `media=` » — puis ne tester que ça. Une nouvelle
   hypothèse **remplace** l'ancienne ; les correctifs ne s'empilent pas.

Le pipeline est un système multi-composants, et il est déjà instrumenté pour ça :
`--debug` écrit `<system>.debug.log` avec la décision de chaque step, `<system>.errors.log`
range les échecs par cause, et la vue *errors* de la TUI en donne le décompte. **On les lit
avant de supposer où ça casse.**

**Trois correctifs échoués veulent dire que la conception est fausse.** Si chaque correctif
découvre un problème ailleurs, ou si chacun réclame « juste un petit remaniement » : on
s'arrête, on ne tente pas un quatrième, et on pose la question d'architecture. Ce n'est plus
une hypothèse fausse, c'est une frontière mal placée.

### Preuve avant affirmation

**Si la commande n'a pas été lancée dans ce message, son résultat ne peut pas être
affirmé.** Avant toute phrase disant que quelque chose passe, marche ou est fini : quelle
commande le prouve ; la lancer en entier, pas une variante restreinte ; lire toute la sortie
**et le code de sortie** ; vérifier que la sortie soutient vraiment l'affirmation — sinon,
énoncer l'état réel, avec la sortie.

| Affirmation | Preuve exigée |
|---|---|
| « les portes passent » | `just ci`, code de sortie **0** lu — la queue verte de `just test` ne dit rien des trois portes qui la précèdent |
| « le binaire que tu testes est à jour » | la reconstruction lancée dans ce message : `just ci` réécrit `target/debug` sous un run en cours, et un binaire périmé ne signale pas qu'il l'est |
| « le paquet Nix build » | `just build-static` — `just ci` compile en **debug**, le paquet en **release** |
| « le bug est corrigé » | le symptôme d'origine rejoué, pas le code relu |
| « plus aucune trace de X » | le grep, zéro résultat, écrit **dans la langue et la tournure du fichier** — un grep bâti sur la formulation attendue ne prouve pas l'absence du concept |
| « ScreenScraper répond ceci » | l'appel réellement passé, et rien de secret imprimé |
| « le rendu est correct » | rien, de ce côté : la TUI n'est pas observable ici (pas de terminal de contrôle). Ça part dans `manual_tests.md` (§4) |

« Ça devrait aller », « le changement est trivial », « clippy est passé » ne prouvent rien :
clippy n'est pas un compilateur et `just ci` n'est pas un `makepkg`. La fatigue, la pression
et une longue série de verts sont les trois moments pour lesquels cette règle existe.

### Relecture de clôture — un contexte frais

Avant de présenter les options d'intégration (§3), une branche portant un changement
**borné ou architectural** (§1) reçoit **une** relecture par un contexte **qui ne l'a pas
écrite**. Relire son propre diff n'est pas cette relecture : même auteur, mêmes angles
morts. Un changement trivial n'y passe pas — il n'a ni plan, ni décision à présenter, et
brûler un contexte sur une coquille est ce que le chemin Trivial existe pour éviter.

Comment obtenir ce contexte, par ordre de préférence :

1. **Un sous-agent relecteur** — le chemin normal. Il lit le diff dans son propre contexte
   et seuls ses constats reviennent. Lancer un sous-agent n'est **pas** une pratique par
   défaut sur rompom : cette relecture en est la seule exception, et elle ne s'étend à rien
   d'autre (§10, `dispatching-parallel-agents` et `subagent-driven-development`).
2. À défaut, une session `/clear`ée, relisant depuis le diff et le plan seuls.

**Demander au mainteneur de regarder n'est pas un substitut** : il est la personne que la
relecture protège, et son feu vert n'est pas une relecture. Si aucun contexte frais n'est
disponible, le dire franchement dans le message de clôture — une auto-relecture est plus
faible, et savoir si ça suffit avant de merger est une décision qui lui revient, prise en
connaissance de cause.

Le relecteur reçoit la plage de diff (`$(git merge-base master HEAD)..HEAD`), le plan, sa
section *Review Focus* verbatim et les décisions du journal. **Jamais l'historique de
session** : ça le mettrait sur le fil du raisonnement au lieu du produit. Puis :

- **Regrader chaque constat par son effet**, pas selon que le plan mentionnait ou non
  l'entrée qui le déclenche. Un plan est un document d'intention ; son silence sur une
  entrée n'autorise pas cette entrée à casser un paquet. Un constat classé mineur parce que
  le plan se taisait a noté le plan, pas l'effet.
- **Critique et Important** prennent **une** passe de correction, chacune vérifiée par sa
  propre reproduction, puis les portes relancées.
- **Mineur** part au journal et au message de clôture comme point différé. Il n'entre pas
  dans la passe — c'est au mainteneur de trancher.
- Un constat délibérément non corrigé **est** une décision, et elle remonte.

Le retour de relecture s'évalue, il ne se joue pas : reformuler le point technique, le
vérifier contre ce dépôt, contredire avec un argument quand il est faux. « Tu as tout à fait
raison » n'est pas une réponse. Et un relecteur peut se tromper — y compris un sous-agent
qui rend un constat assuré sur du code qu'il a mal lu.

### Le journal de bord

Un chantier qui traverse plusieurs sessions perd son contexte à la compaction. Le plan dit
l'intention ; le **journal** dit ce qui s'est réellement passé, et c'est la seule chose qui
survit.

**Un journal par fichier de plan**, à côté de lui et **commité** :

| Fichier de plan | Son journal |
|---|---|
| `.claude/plans/v{X.Y.Z}/plan.md` | `.claude/plans/v{X.Y.Z}/progress.md` |
| `.claude/plans/process/<nom>/plan.md` | `.claude/plans/process/<nom>/progress.md` |

Un changement **borné** n'a pas de plan, donc pas de journal : ses décisions vont dans le
message de clôture, qui devient alors le seul compte rendu — une raison de plus pour que le
cliquet de §1 penche vers le chemin lourd quand un changement risque de grossir.

La première ligne nomme le plan suivi. Exemple :

```markdown
# Journal — plan : .claude/plans/v0.23.0/plan.md

Pré-vol : l'étape 3 consomme le champ que l'étape 1 ajoute au `Rom` — les noms concordent.
Étape 1 : terminée (c485656, `just ci` → 0).
Étape 2 : Décision : le sha1 des disques 2+ est stocké dans une liste et non dans une map —
  l'ordre porte le numéro de disque — coût si c'est faux : une migration de `state.yml`.
Étape 2 : terminée (a1b2c3d, `just ci` → 0).
Final : mineur (différé) : `read_pkgver()` mériterait un test sur un PKGBUILD tronqué.
```

Les règles :

- **On statue, on ne bloque pas.** Un conflit dans le plan, une ambiguïté, un défaut du
  plan : on tranche, on écrit `Décision : <ce qui est décidé> — <pourquoi> — <ce que ça coûte
  si c'est faux>`, et on continue. Une entorse au plan sans décision écrite est une décision
  prise en secret.
- **Cinq choses arrêtent le travail** au lieu d'être tranchées : une opération destructrice
  ou irréversible ; un point touchant à un secret, les credentials ScreenScraper en tête ;
  une commande git interactive, sous la règle du lot de §3 ; un plan si cassé que toute suite
  est une devinette ; et **trois correctifs échoués** sur le même problème, qui est une
  question d'architecture et jamais une quatrième tentative.
- **La ligne s'écrit dans le même message que le commit**, pas plus tard. La compaction
  n'attend pas un moment commode.
- **Après une compaction, le journal et `git log` font foi, pas le souvenir.** Une étape qui
  porte une ligne `terminée` est faite ; on reprend à la première qui n'en a pas.
- Chaque `Décision :` et chaque mineur différé est **répété dans le message de clôture**.
  C'est le seul endroit où ces décisions atteignent le mainteneur.

---

## 9. Gabarit de plan

Ce gabarit sert le chemin **architectural** de §1. Un changement borné n'a pas de fichier de
plan, un trivial n'a rien du tout.

```markdown
## Plan : [Titre]

**Type :** [pipeline | package | ui | conf | state | bug | refactor | doc | process]
**Étage(s) :** [conf | collect | pipeline | package | state | ui | nix | doc]
**Objectif :** une phrase — ce que ça construit.
**Pourquoi :** d'où on part, ce qui ne va pas.
**Architecture :** deux ou trois phrases sur l'approche retenue.
**Spec :** le document dont ce plan dérive, ou « aucune — ce plan est sa propre spec » (§1).

### Contraintes globales
Les exigences valables pour **toutes** les étapes, une ligne chacune, valeurs exactes :
versions planchers, fichiers interdits au diff, formats à ne pas casser, décisions déjà
arrêtées avec le mainteneur. Chaque étape les porte implicitement.

### Review Focus
Les modes de défaillance que les tests de ce plan **n'exercent pas** et qui mordraient un
utilisateur : une ligne chacun, le plus probable d'abord. Écrits une fois, au moment du
plan ; relus tels quels à la clôture (§8). Une section vide veut dire qu'on a cherché et
n'a rien trouvé, pas qu'on a sauté la recherche.

### Fichiers touchés
- [ ] `src/...`
- [ ] `assets/templates/...`
- [ ] `CLAUDE.md` / `README.md`

### Étapes atomiques
#### Étape 1 : [Titre]
**Description :** ...
**Produit :** ce sur quoi les étapes suivantes s'appuient — noms exacts de fonctions, de
champs du `Rom`, de sections.
**Consomme :** ce que cette étape prend à une précédente, aux mêmes noms exacts.
**Vérification :** `just ci` (ou la cible pertinente)
**Commit :** `type(scope): message`

### Portes de qualité
- [ ] `just ci` passe
- [ ] Tests ajoutés pour le code pur touché (§4)
- [ ] Doc synchronisée (même commit)
- [ ] Commits atomiques, scope réel (jamais `all`), sujets de qualité changelog
- [ ] Branche dédiée ; clôture en présentant les options d'intégration (§3)
- [ ] Relecture de clôture par un contexte frais, constats regradés par effet (§8)
- [ ] Journal tenu ; décisions et mineurs différés répétés dans le message de clôture (§8)
```

### Pourquoi *Consomme* / *Produit*

Chaque step du pipeline alimente les suivants **par le `Rom`, pas par le disque** — c'est
l'invariant que `CLAUDE.md` documente, et celui dont dépend toute la logique de reprise. Un
plan qui ajoute ou déplace un step doit donc déclarer ce qu'il dépose dans le `Rom` et ce
qu'il y lit : « l'étape 2 remplit `rom.medias`, l'étape 3 le consomme » devient une
dépendance déclarée plutôt qu'une chose à se rappeler quatre sessions plus tard.

Un **balayage pré-vol**, avant la première étape, la vérifie : une ligne de journal par
couple producteur / consommateur, avec ce qu'on a trouvé. Les étapes qui ne partagent rien
n'ont pas de ligne ; un plan dont aucune étape ne partage rien porte la seule ligne
`Pré-vol : aucune interface partagée`.

### Pas de réservé

Une étape contient ce qu'il faut pour la mener, pas la promesse de le trouver. Sont des
**défauts de plan** : « TBD », « à compléter », « gérer les cas limites », « ajouter la
validation appropriée », « comme l'étape 2 » sans répéter le contenu, et toute étape qui dit
quoi faire sans montrer comment.

---

## 10. Arbitrage des skills `superpowers`

Le plugin `superpowers` injecte son skill `using-superpowers` dans **chaque** session — au
démarrage, après un `/clear`, après chaque compaction — enveloppé dans
`<EXTREMELY_IMPORTANT>`, avec la règle « s'il y a ne serait-ce qu'1 % de chance qu'un skill
s'applique, tu DOIS l'invoquer ». Le plugin énonce aussi que les instructions utilisateur
priment sur ses skills. La table ci-dessous **est** cette primauté, rendue explicite.
**Là où un skill et ce document ne disent pas la même chose, ce document l'emporte.**
Arbitré contre `superpowers` 6.4.1.

Ce que les verdicts obligent :

| Verdict | Sens opératoire |
|---|---|
| **Adopté** | Suivre le skill tel qu'il est écrit. Le lire quand la situation se présente. |
| **Adapté** | La forme du skill tient, mais c'est la version de **ce** document qui lie — y compris là où elle est plus étroite. Lire ce document, pas le skill. |
| **Réduit** | Seule la partie nommée ici s'applique. Tout le reste du skill — ses portes, ses artefacts, ses étapes supplémentaires — ne s'applique pas. |
| **Rejeté par défaut** | Ne pas l'utiliser sans demande explicite du mainteneur. |
| **Rejeté** | Ne pas l'utiliser. |
| **Surclassé** | Ses instructions ne lient pas ici. |
| **Marginal** | Hors de la pratique courante d'ici. Autorisé dans le cas étroit que sa ligne décrit, et dans aucun autre. |

| Skill | Verdict | Pourquoi |
|---|---|---|
| `systematic-debugging` | **Adapté** | Sa discipline est reprise en [§8 → Cause avant correctif](#cause-avant-correctif), et c'est **§8 qui lie**. Le skill impose par ailleurs un test qui échoue avant tout correctif, pour n'importe quel incident : §4 en dispense ce qui n'est pas testable ici, et cette dispense gagne. Rien n'était écrit sur le débogage, sur un projet dont l'historique de bugs est une suite de symptômes traités avant leur cause. |
| `verification-before-completion` | **Adapté** | Repris en [§8 → Preuve avant affirmation](#preuve-avant-affirmation), avec la table de preuves de ce dépôt — c'est elle qui lie, pas les exemples du skill. §7 disait **quoi** vérifier, jamais **quand**. |
| `receiving-code-review` | **Adopté** | Évaluer le retour, le vérifier contre ce dépôt, contredire avec un argument. Pas d'acquiescement de façade. |
| `finishing-a-development-branch` | **Adapté** | Suite verte avant de présenter, base confirmée, options d'intégration présentées et non choisies. Deux écarts qui lient : la porte est `just ci` à 0 et non `cargo test` (§8), et son option « garder la branche telle quelle » n'existe pas ici — §3 interdit de laisser une branche terminée non mergée. Sous la règle du lot de §3, qui remplace l'exécution silencieuse du menu. |
| `test-driven-development` | **Adapté** | L'Iron Law lie le code pur et rien d'autre ; test et correctif dans le même commit. Cf. [§4](#ce-que-la-loi-des-tests-lie-et-ce-quelle-ne-lie-pas). |
| `writing-plans` | **Adapté** | En-tête, Contraintes globales, Review Focus, blocs Consomme/Produit et règle « pas de réservé » : dedans (§9). Son emplacement `docs/superpowers/plans/` : non, les plans vivent sous `.claude/plans/`. Sa granularité « une étape = un commit » : non, elle produirait des commits rouges (§4). |
| `executing-plans` | **Adapté** | Le journal et « on statue, on ne bloque pas » : dedans (§8). Son espace de travail `.superpowers/sdd/` et ses scripts : non — le journal est commité à côté de son plan. Et il **ne se supprime pas** en fin de chantier, contrairement à ce que le skill ordonne : c'est le compte rendu, pas du scratch. |
| `requesting-code-review` | **Adapté** | Le principe du contexte frais et la règle « jamais l'historique de session » : dedans. Sa cadence — une relecture par tâche et avant chaque merge — non : **une** relecture à la clôture de branche (§8). |
| `brainstorming` | **Réduit** | Seul son triage est repris, devenu [§1](#1-dimensionner-un-changement-puis-créer-le-plan), avec un chemin **Trivial** qu'il n'a pas. Pas de document de spec séparé : `plan.md` est la spec. Pas de compagnon visuel dans le navigateur. |
| `subagent-driven-development` | **Rejeté par défaut** | Un implémenteur plus un relecteur par étape, chacun relisant `CLAUDE.md` depuis zéro, ne se rentabilise pas à la taille des chantiers d'ici. Disponible sur demande ; à **proposer** quand un plan dépasse une petite dizaine d'étapes. La relecture de clôture est gardée dans tous les cas. |
| `using-git-worktrees` | **Rejeté** | Un second worktree ne partage pas `target/` : la première porte y est une reconstruction à froid de tout l'arbre de dépendances. `.direnv` est perdu, donc `nix develop` réévalue. Et son étape 2 lance `cargo build` dès qu'un `Cargo.toml` est à la racine — ce qui est le cas — hors `nix develop`, donc avec la mauvaise toolchain. La branche dédiée de §3 **est** l'isolation. Ceci rejette le workflow manuel du skill, pas un worktree natif que le mainteneur demanderait. |
| `using-superpowers` | **Surclassé** | Son « 1 % de chance → tu DOIS l'invoquer » ne bat pas une ligne de cette table. C'est la raison pour laquelle cette table existe. |
| `dispatching-parallel-agents` | **Rejeté par défaut** | Du travail réellement indépendant est rare ici : le pipeline est un DAG, et ses étages se lisent ensemble. Surtout, §8 ne concède qu'**une** exception au fait que lancer un sous-agent n'est pas la pratique par défaut, et c'est la relecture de clôture — un skill dont l'objet est d'en lancer plusieurs ne peut pas être « autorisé » à côté. |
| `writing-skills` | **Marginal** | rompom n'écrit aucun skill, et ceux qui traînaient sous `.claude/skills/` n'étaient pas les nôtres. Aucune porte de ce dépôt ne peut donc vérifier cette règle. Si un skill rompom voit le jour, le verdict devient **Adopté** pour lui. |
| `diagnosing-superpowers` | **Marginal** | Rien à voir avec rompom : il ne sert qu'à construire un rapport de bug à l'intention des mainteneurs du plugin, et n'a pas d'autre usage ici. |

`CLAUDE.md` porte la **même** table, condensée et groupée par verdict, parce que c'est le
seul fichier réinjecté à chaque session. Écrire deux fois, c'est accepter qu'elles
divergent : en cas de divergence, **celle-ci fait foi**.

### OpenSpec n'est pas le processus de rompom

`openspec/`, `.claude/commands/opsx/` et six skills `openspec-*` avaient été installés dans
ce dépôt et n'ont **jamais servi** : configuration par défaut sans une ligne de contexte
projet, `specs/` et `changes/` vides, rien de suivi par git, aucun commit. Ils doublaient
`brainstorming` / `writing-plans` / `executing-plans` avec un jeu d'artefacts plus lourd,
alors que la convention de plans de §1 tourne depuis v0.16. Retirés le 2026-09-27.

**Le processus de planification de rompom est §1 + §9, et rien d'autre.** À noter, parce que
la suppression ne suffit pas : les skills `opsx:*` fournis par un **plugin** restent listés
à chaque session quoi qu'on retire de ce dépôt. C'est cette ligne qui tranche, pas le `rm` —
et une réinstallation d'OpenSpec ici serait une décision à prendre avec le mainteneur, pas
un effet de bord d'outillage.

---

## 11. Ce qui ne change pas entre les versions

- **Un `description.xml` par paquet ROM.** Le `gamelist.xml` est régénéré par hook
  post-install Batocera — rompom ne l'écrit jamais.
- **Vérification SHA1 systématique** avant de considérer un fichier comme présent.
- **L'état est incrémental** : `state.yml` fait foi pour décider ce qui a changé ; un
  `pkgver` ne se bump que si ROM, média ou `description.xml` a réellement changé.
- **Le pipeline est un DAG** de steps avec `wait_for`/`next` — pas de séquence codée en dur.
- **Les credentials ScreenScraper sont un secret** : jamais dans un log, un test, une
  fixture ou un message d'erreur.
- **Git** : branche dédiée et commits atomiques, toujours. Claude mène le flux complet, en
  annonçant le lot de commits et en demandant confirmation avant chaque `fetch`, `pull`,
  `push` et `tag` (§3).
- **Nix** : toujours `nix develop --command …` pour les commandes non interactives.
- **`tmp/`** : scratch non commité (handoffs design, notes, sorties de travail).
- **`CHANGELOG.md` est généré**, `CHANGELOG-legacy.md` est figé.
- **Le `Justfile` est la seule définition des portes.** pre-commit et la CI l'appellent ;
  on n'ajoute jamais une vérification dans la CI sans l'ajouter au Justfile.

---

**Dernière mise à jour :** 2026-09-27
**Statut :** Actif
