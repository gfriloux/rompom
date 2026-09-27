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
classement à voix haute** pour que l'utilisateur puisse le renverser, puis suivre ce
chemin-là.

| Chemin | Ce que c'est | Artefact |
|---|---|---|
| **Trivial** | Aucune décision à présenter : une coquille, un lien mort, une reformulation qui ne change ni comportement ni interface. | Aucun, et pas d'accord préalable. On le fait et on dit ce qu'on a fait. |
| **Spike** | Une question de faisabilité — « l'API ScreenScraper renvoie-t-elle ce champ ? », « ce `.chd` multi-piste est-il lisible ? ». La sortie est une **réponse**, pas du code qu'on garde. | Aucun. Énoncer la question et la sonde en deux phrases, obtenir un feu vert, puis chercher au moins cher que la justesse permette. Rapporter une recommandation ; ce qui a été construit est étiqueté jetable. |
| **Bounded** (borné) | Un changement bien cadré sur du code **déjà présent ici** : un drapeau CLI, une correction dans un handler, un champ de plus dans `description.xml`, un ajustement de template. | Pas de fichier de plan. Présenter un design court en conversation, obtenir un **oui explicite**, puis implémenter. |
| **Architectural** | Un nouveau step de pipeline, un changement de format d'état, une refonte de l'UI, un nouveau type de source, tout ce qui déplace une responsabilité entre étages ou change le DAG. | Un plan sous `.claude/plans/`, relu par l'utilisateur **avant** implémentation. |

Les noms *Spike*, *Bounded* et *Architectural* sont ceux du plugin `superpowers`
(cf. §Arbitrage des skills), volontairement laissés en anglais : un lecteur qui part de ce
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

Dès qu'une version ou un chantier architectural est décidé, créer :

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
| Nouveau step de pipeline | `pipeline` | 1. step + handler (`feat(pipeline): …`) → 2. test → 3. MAJ du DAG dans `CLAUDE.md` |
| Nouveau template PKGBUILD | `package` | 1. template + sélection (`feat(package): …`) → 2. snapshot → 3. doc |
| Changement de `description.xml` | `package` | 1. snapshot mis à jour (`test(package): …`) → 2. impl → 3. doc |
| Changement d'UI / phase | `ui` | 1. impl (`feat(ui): …`) → 2. `manual_tests.md` → relecture visuelle |
| Correction de bug | étage concerné | test de régression **+** fix dans le **même** commit (`fix(scope): …`) — cf. note ci-dessous |
| Refactor | étage concerné | 1. refactor sans changer de snapshot (`refactor(scope): …`). Si un snapshot bouge, ce n'était pas un refactor. |
| Changement de format d'état | `state` | migration explicite + note de rupture ; jamais de changement silencieux de `state.yml` |
| Doc seule | — | `docs: …` |

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

**Quatre règles**, quel que soit le type de changement : deux gouvernent la façon de mener
une étape, deux la façon de clore une branche. La cinquième, la loi des tests, vit en §4 —
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

**Trois correctifs échoués veut dire que la conception est fausse.** Si chaque correctif
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

Avant de présenter les options d'intégration (§3), la branche reçoit **une** relecture par
un contexte **qui ne l'a pas écrite**. Relire son propre diff n'est pas cette relecture :
même auteur, mêmes angles morts.

Comment obtenir ce contexte, par ordre de préférence :

1. **Un sous-agent relecteur** — le chemin normal. Il lit le diff dans son propre contexte
   et seuls ses constats reviennent. C'est la **seule dérogation** à la règle de session qui
   interdit de lancer un agent sans demande explicite, et elle ne vaut **que** pour cette
   relecture.
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
- [ ] Branche dédiée ; clôture en présentant les options d'intégration (§3)
```

---

## 10. Ce qui ne change pas entre les versions

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

**Dernière mise à jour :** 2026-08-09
**Statut :** Actif
