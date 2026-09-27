# Plan : arbitrer superpowers sur rompom

**Type :** méthodes de travail (process/outillage, pas une fonctionnalité produit)
**Étage(s) :** `doc`
**Objectif :** décider, une fois et par écrit, quelles parties du plugin `superpowers`
gouvernent le travail sur rompom et lesquelles sont surclassées — pour que le hook
`SessionStart` du plugin ne rouvre pas la question à chaque session.

**Pourquoi :** `superpowers` 6.4.1 réinjecte le texte intégral de son skill
`using-superpowers` à chaque démarrage, `/clear` et compaction (matcher
`startup|clear|compact`), sous `<EXTREMELY_IMPORTANT>`, avec la règle « s'il y a ne
serait-ce qu'1 % de chance qu'un skill s'applique, tu DOIS l'invoquer ». Plusieurs de
ses skills contredisent frontalement les normes de rompom : son Iron Law TDD ne peut
pas lier les handlers réseau ni le rendu ratatui, ses emplacements de plans ignorent
`.claude/plans/`, et son menu de clôture de branche suppose un flux git que rompom
vient tout juste de changer. Le plugin tranche lui-même en notre faveur — « User
instructions (CLAUDE.md, AGENTS.md, etc.) take precedence over skills » — mais cette
déférence n'opère que si l'arbitrage est effectivement écrit.

**Architecture :** les règles détaillées vivent dans `PROCEDURE_PLANS.md`, déjà le point
d'entrée unique du processus. `CLAUDE.md` porte la table condensée, pour que le verdict
soit en contexte dès le premier token de chaque session, compaction comprise.

**Spec :** aucune — ce plan est sa propre spec. Matière première :
`~/.claude/plugins/cache/claude-plugins-official/superpowers/6.4.1/skills/` (15 skills)
et l'arbitrage rendu sur le dépôt voisin `../stc`
(`.claude/plans/release/superpowers-arbitrage.md` et son journal).

## Contraintes globales

- **Aucun fichier `src/`, `assets/` ou `.nix` dans le diff.** Ce chantier ne touche que
  de la documentation de processus. `just ci` doit rester vert, mais ne prouve rien ici.
- **Les sujets de commit partent au changelog.** `cliff.toml` route `^docs` vers la
  section *Documentation* : un commit = une ligne de note de version, rédigée comme
  telle, en anglais (convention `PROCEDURE_PLANS.md` §3).
- **Les plans historiques sous `.claude/plans/v*/` ne sont pas réécrits.** Ce sont des
  archives de chantiers menés sous la norme en vigueur à l'époque ; les corriger
  falsifierait le compte rendu. Seuls `PROCEDURE_PLANS.md` et `CLAUDE.md` font foi sur
  la norme courante.
- **Les quatre décisions prises avec l'utilisateur le 2026-09-27 sont des entrées
  fermées, pas des questions ouvertes :**
  1. Git : Claude mène le flux git complet, sous la règle du lot (étape 2).
  2. OpenSpec : retiré (étape 8).
  3. Relecture de clôture : un sous-agent relecteur est autorisé, et uniquement là.
  4. Emplacement : `PROCEDURE_PLANS.md` pour les règles, `CLAUDE.md` pour la table.
- **Les noms des chemins de dimensionnement restent ceux du plugin** — *Spike*,
  *Bounded*, *Architectural* — avec leur glose française. Un lecteur qui grep le plugin
  depuis notre doc doit retomber dessus ; `../stc` a dû renommer après coup pour cette
  raison exacte.
- **`tmp/`** reste du scratch non commité. Le journal de bord de ce chantier, lui, est
  commité : `.claude/plans/process/superpowers-arbitrage/progress.md`.

## Review Focus

Modes de défaillance que ce changement ne doit pas introduire, vérifiés délibérément à
la clôture :

- **Une règle qui se lit comme un conseil et non comme une porte.** Un arbitrage qui ne
  dit pas « celui-ci l'emporte » perd contre le cadrage `<EXTREMELY_IMPORTANT>` du hook.
- **Un skill non arbitré.** Les 15 doivent figurer dans la table, y compris ceux dont le
  verdict est « marginal ici ».
- **Une trace survivante de l'ancienne politique git.** « Claude ne fait jamais merge,
  push ni tag » apparaît dans `PROCEDURE_PLANS.md` §3 **et** §9, dans les garde-fous de
  `CLAUDE.md`, et dans les plans historiques. Le grep de vérification doit matcher la
  **langue du fichier** : la phrase est en français ici, un grep anglais la rate.
- **La règle du lot écrite comme si elle couvrait `push`.** La mesure dit l'inverse : la
  clé GitHub est une `ED25519-SK`, un contact physique par connexion, qu'aucun cache
  n'évite — alors que la signature GPG est une phrase de passe mise en cache par
  `gpg-agent`. Confondre les deux produit une règle qui promet ce qu'elle ne tient pas.
- **Une loi des tests qui mord le code non testable.** Si l'Iron Law est écrite assez
  largement pour couvrir les handlers, la modale ou le rendu ratatui, elle produira des
  tests rituels — et `PROCEDURE_PLANS.md` §4 range explicitement ces points en manuel.
- **Une table écrite deux fois qui diverge.** `PROCEDURE_PLANS.md` et `CLAUDE.md`
  porteront les mêmes verdicts ; il faut dire noir sur blanc laquelle fait foi.
- **Un sujet de commit qui ne se lit pas comme une note de version**, alors que `^docs`
  part au changelog.

## Fichiers touchés

- [ ] `PROCEDURE_PLANS.md` — étapes 1 à 8
- [ ] `CLAUDE.md` — étapes 2 et 7
- [ ] `.claude/plans/process/superpowers-arbitrage/phase0_results.md` — étape 0
- [ ] `.claude/plans/process/superpowers-arbitrage/progress.md` — toutes les étapes
- [ ] Suppressions (non suivies par git, donc hors diff) : `openspec/`,
      `.claude/commands/opsx/`, `.claude/skills/openspec-*` — étape 8

---

## Étapes atomiques

### Étape 0 : Phase 0 — audit

**Description :** `PROCEDURE_PLANS.md` §2 impose de vérifier l'état réel du dépôt avant
de commencer. Lancer `nix develop --command just ci` et consigner la sortie.

**Produit :** `phase0_results.md`, et la certitude que le vert de fin de chantier n'est
pas hérité d'un rouge préexistant.
**Consomme :** rien.

**Vérification :** `just ci` → sortie 0, consignée verbatim.

**Commit :** aucun (fichier de plan, commité avec l'étape 1).

---

### Étape 1 : Dimensionner un changement

**Description :** nouvelle section `§ Dimensionner un changement` important le triage de
`brainstorming`, plus un chemin **Trivial** que le skill n'a pas. Quatre chemins :

| Chemin | Ce que c'est | Artefact |
|---|---|---|
| **Trivial** | Aucune décision à présenter : une coquille, un lien mort, une reformulation qui ne change ni comportement ni interface. | Aucun, et pas d'accord préalable. On le fait et on dit ce qu'on a fait. |
| **Spike** | Une question de faisabilité — « l'API SS renvoie-t-elle X ? », « ce format est-il lisible ? ». La sortie est une réponse, pas du code qu'on garde. | Aucun. Énoncer la question et la sonde en deux phrases, puis rapporter une recommandation. Ce qui a été construit est étiqueté jetable. |
| **Bounded** (borné) | Un changement bien cadré sur du code **déjà présent ici** : un drapeau CLI, une correction dans un handler, un ajustement de template. | Pas de fichier de plan. Présenter un design court en conversation, obtenir un oui explicite, puis implémenter. |
| **Architectural** | Un nouveau step de pipeline, un nouveau format d'état, une refonte de l'UI, tout ce qui déplace une responsabilité entre étages ou change le DAG. | Un plan sous `.claude/plans/`, relu par l'utilisateur avant implémentation. |

Trois règles accompagnent la table : **borné se mesure au dépôt, pas à la familiarité**
(si le flux qu'on change n'est pas déjà là à lire, ce n'est pas borné) ; **le cliquet est
à sens unique** (dans le doute, le chemin le plus lourd ; une complexité découverte en
cours de route fait monter d'un cran, jamais descendre) ; **pas de spec séparée** — pour
un changement architectural, `plan.md` **est** la spec.

Cette section amende §1, qui exige aujourd'hui un plan validé pour tout changement, y
compris une coquille. §1 renvoie désormais au dimensionnement d'abord, et n'impose le
dossier de plan qu'au chemin architectural.

Elle étend aussi l'arborescence de §1 : un chantier de process n'est pas une version,
d'où `.claude/plans/process/<nom>/` à côté des `v{X.Y.Z}/` — ce plan en est le premier
usage.

**Produit :** le titre `§ Dimensionner un changement` et les quatre noms de chemin.
**Consomme :** rien.

**Fichiers :** `PROCEDURE_PLANS.md` (nouvelle section + amendement de §1),
`.claude/plans/process/superpowers-arbitrage/` (plan + phase 0).

**Vérification :** §1 ne réclame plus un plan pour tout ; les quatre noms de chemin sont
ceux du plugin pour trois d'entre eux ; la numérotation des sections suivantes reste
cohérente.

**Commit :** `docs(procedure): size a change before planning it`

---

### Étape 2 : Politique git — Claude mène le flux, sous la règle du lot

**Description :** réécriture de §3, et des mentions correspondantes en §6 (release) et §9
(« ce qui ne change pas »), plus les garde-fous de `CLAUDE.md`.

Ce qui change : Claude exécute désormais le flux git complet — `commit`, `merge`, `tag`,
`push` — et clôt une branche en présentant les options d'intégration plutôt qu'en
décidant seul. Ce qui ne change pas : la branche dédiée, les commits atomiques en
Conventional Commits, le scope réel, et le fait que chaque commit passe les portes seul.

La règle qui encadre l'interactivité, écrite depuis la configuration mesurée et non
supposée :

- **`commit` et `tag` sont signés OpenPGP** par une clé ed25519 **sur disque** protégée
  par phrase de passe (`commit.gpgsign=true`, `tag.gpgsign=true`). `gpg-agent` la met en
  cache — aucun `gpg-agent.conf` ne fixe de TTL, donc les défauts gpg : 10 min
  d'inactivité, 2 h au maximum. Un lot enchaîné dans cette fenêtre coûte **une** saisie.
  `pinentry-qt` est configuré : la boîte de dialogue s'ouvre sur le bureau de
  l'utilisateur, pas dans le terminal de Claude — ce qui rend l'opération possible ici,
  où `/dev/tty` est inouvrable.
- **`fetch`, `pull` et `push` passent par SSH avec une clé `ED25519-SK`** — une YubiKey.
  Chaque connexion demande un **contact physique**, qu'aucun cache n'évite.

D'où deux cadences distinctes :

1. **Annoncer le lot de commits et attendre le feu vert avant le premier**
   (« 5 commits atomiques arrivent — tu es au clavier ? »), puis les enchaîner en
   s'appuyant sur le cache.
2. **Demander confirmation avant chaque `fetch`, `pull`, `push` et `tag`** : un contact
   par connexion pour les trois premiers, et un tag peut tomber hors fenêtre de cache.

Et une interdiction : **si une saisie traîne ou qu'une signature échoue, on s'arrête et
on le dit.** Jamais de `--no-gpg-sign`, jamais de contournement d'une signature absente.

**Produit :** le nom « règle du lot » et la distinction lot / par-commande.
**Consomme :** rien.

**Fichiers :** `PROCEDURE_PLANS.md` (§3, §6, §9), `CLAUDE.md` (garde-fous).

**Vérification :** `grep -niE "jamais.*(merge|push|tag)|ne fait jamais"
PROCEDURE_PLANS.md CLAUDE.md` ne renvoie plus aucune règle normative — grep écrit en
**français**, la langue des fichiers. Les plans historiques sous `.claude/plans/v*/` sont
hors périmètre et le restent.

**Commit :** `docs(procedure): let Claude run the git flow under the interactive batch rule`

---

### Étape 3 : Cause avant correctif, preuve avant affirmation

**Description :** ouvre `§ Discipline d'exécution` avec les deux règles qui mordent le
plus souvent, adaptées de `systematic-debugging` et `verification-before-completion`.

**Cause avant correctif.** Pas de correctif avant que la cause soit comprise ; un
correctif qui fait disparaître le symptôme sans explication est un échec, même quand
l'erreur disparaît. Cinq étapes : lire l'erreur en entier ; reproduire au plus étroit
(descendre de `just ci` au test unitaire, ou du run complet à un seul ROM) ; regarder ce
qui a changé (`git diff`, les derniers commits, une dépendance bumpée) ; comparer à ce
qui marche ; une hypothèse, un changement minimal — les correctifs ne s'empilent pas.
Le pipeline est un système multi-composants au sens de la phase 1 du skill, et `--debug`,
`<system>.debug.log` et `<system>.errors.log` en sont déjà l'instrumentation : on les
lit avant de supposer. **Trois correctifs échoués veut dire que la conception est
fausse** — on s'arrête, on ne tente pas un quatrième, et on pose la question d'archi.

**Preuve avant affirmation.** Si la commande n'a pas été lancée dans ce message, son
résultat ne peut pas être affirmé. Avec la table de preuves propre à rompom :

| Affirmation | Preuve exigée |
|---|---|
| « les portes passent » | `just ci` → sortie 0, lue en entier |
| « le binaire testé est à jour » | la reconstruction lancée dans ce message — `just ci` réécrit `target/debug` sous un run en cours, et un binaire périmé ment sans le dire |
| « le paquet Nix build » | `just build-static` — `just ci` compile en debug, le paquet en release |
| « le bug est corrigé » | le symptôme d'origine rejoué, pas le code relu |
| « plus aucune trace de X » | le grep, zéro résultat, **écrit dans la langue du fichier** |
| « ScreenScraper répond ça » | l'appel réellement passé, et rien de secret imprimé |

**Produit :** le titre `§ Discipline d'exécution` et ses deux premières sous-sections.
**Consomme :** rien.

**Fichiers :** `PROCEDURE_PLANS.md`.

**Vérification :** les six lignes de la table de preuves nomment des commandes qui
existent dans le `Justfile` ou dans le CLI ; la table de preuves ne contredit pas §7.

**Commit :** `docs(procedure): require a root cause before a fix and evidence before a claim`

---

### Étape 4 : La loi des tests

**Description :** amende §4 pour dire ce que `test-driven-development` oblige ici et ce
qu'il n'oblige pas. Contrairement au dépôt voisin `../stc`, dont les tests bootent une
VM, rompom a de vrais tests unitaires dans `just test` : l'Iron Law **tient**, mais pas
partout.

- **Elle lie le code pur.** Pas de fonction pure nouvelle sans son test écrit d'abord et
  **regardé échouer** — un test qui n'a jamais échoué ne prouve rien.
- **Elle ne lie pas** ce que §4 range déjà en manuel : les appels ScreenScraper /
  Internet Archive, les handlers qui en dépendent, le rendu ratatui, la modale
  d'identification, `makepkg` sur Batocera. Un test écrit pour satisfaire le rituel sur
  ces points ne teste rien. Ils partent dans `manual_tests.md`.
- **Le test et le correctif vont dans le même commit.** C'est une entorse assumée à la
  granularité par étape de `writing-plans`, qui commiterait le test rouge séparément :
  §3 exige que chaque commit passe les portes seul, un commit rouge casse `git bisect`.
  Le message de commit porte alors la preuve, en décrivant ce que le test produisait
  avant le correctif. Règle déjà écrite en §5, ici rattachée explicitement à la loi.
- **Un snapshot qui bouge par accident reste un blocage dur.**

**Produit :** la formulation « l'Iron Law lie le code pur » et la liste de ce qu'elle ne
lie pas.
**Consomme :** rien de ce plan ; s'appuie sur §4 et §5 existants.

**Fichiers :** `PROCEDURE_PLANS.md` (§4 amendée, renvoi depuis `§ Discipline
d'exécution`).

**Vérification :** la liste des exclusions correspond exactement à celle déjà écrite en
§4 — aucune n'est ajoutée ni retirée au passage ; §5 et §4 disent la même chose sur le
commit unique test + correctif.

**Commit :** `docs(procedure): state what the TDD iron law binds and what it does not`

---

### Étape 5 : Relecture de clôture et journal de bord

**Description :** les deux règles par branche, qui ferment `§ Discipline d'exécution`.

**Relecture de clôture — un contexte frais.** Avant de présenter les options
d'intégration, la branche reçoit **une** relecture par un contexte qui ne l'a pas
écrite ; relire son propre diff n'est pas cette relecture. Le chemin normal est un
sous-agent relecteur : c'est **la seule dérogation** à la règle de session qui interdit
de lancer un agent sans demande explicite, et elle ne vaut que là. À défaut, une session
`/clear`ée, à partir du diff et du plan seuls. **Demander à l'utilisateur de regarder
n'est pas un substitut** : il est la personne que la relecture protège, son feu vert
n'est pas une relecture. Si aucun contexte frais n'est disponible, le dire franchement
dans le message de clôture.

Le relecteur reçoit la plage de diff (`$(git merge-base master HEAD)..HEAD`), le plan, sa
section *Review Focus* verbatim et les décisions du journal — **jamais** l'historique de
session. Puis : **regrader chaque constat par son effet**, pas par le fait que le plan
mentionnait ou non l'entrée qui le déclenche ; Critique et Important prennent **une**
passe de correction, chacune vérifiée par sa propre reproduction, puis les portes
relancées ; Mineur part au journal et au message de clôture, pas dans la passe. Un
constat délibérément non corrigé **est** une décision, et elle remonte à l'utilisateur.

Le retour de relecture s'évalue, il ne se joue pas : reformuler le point technique, le
vérifier contre ce dépôt, contredire avec un argument quand il est faux. « Tu as tout à
fait raison » n'est pas une réponse.

**Le journal de bord.** Un chantier qui traverse plusieurs sessions perd son contexte à
la compaction. Le plan dit l'intention ; le **journal** dit ce qui s'est réellement
passé, et c'est la seule chose qui survit. **Un journal par fichier de plan**, à côté de
lui et commité :

| Fichier de plan | Son journal |
|---|---|
| `.claude/plans/v{X.Y.Z}/plan.md` | `.claude/plans/v{X.Y.Z}/progress.md` |
| `.claude/plans/process/<nom>/plan.md` | `.claude/plans/process/<nom>/progress.md` |

Un changement **borné** n'a pas de plan, donc pas de journal : ses décisions vont dans le
message de clôture, qui devient alors le seul compte rendu — une raison de plus pour que
le cliquet penche vers le chemin lourd quand un changement risque de grossir.

Règles : **on statue, on ne bloque pas** — un conflit dans le plan, une ambiguïté, un
défaut : on tranche, on écrit `Décision : <ce qui est décidé> — <pourquoi> — <ce que ça
coûte si c'est faux>`, et on continue ; une entorse au plan sans décision écrite est une
décision prise en secret. **Cinq choses arrêtent le travail** au lieu d'être tranchées :
une opération destructrice ou irréversible ; un point touchant à un secret (les
credentials ScreenScraper en tête) ; une commande git interactive, sous la règle du lot
de l'étape 2 ; un plan si cassé que toute suite est une devinette ; et trois correctifs
échoués sur le même problème, qui est une question d'archi et jamais une quatrième
tentative. **La ligne s'écrit dans le même appel que le commit**, pas plus tard — la
compaction n'attend pas un moment commode. **Après une compaction, le journal et
`git log` font foi, pas le souvenir.**

**Produit :** le chemin `progress.md` et le format de la ligne `Décision :`.
**Consomme :** de l'étape 1, l'existence de `.claude/plans/process/<nom>/` ; de l'étape 2,
la règle du lot, citée parmi les cinq arrêts.

**Fichiers :** `PROCEDURE_PLANS.md`.

**Vérification :** le chemin du journal correspond à l'arborescence écrite en §1 à
l'étape 1 ; les cinq arrêts sont bien cinq et incluent l'arrêt à trois correctifs
introduit à l'étape 3 ; la dérogation sous-agent est nommée comme dérogation, pas comme
règle générale.

**Commit :** `docs(procedure): close a branch with one fresh review and a committed ledger`

---

### Étape 6 : Gabarit de plan étendu

**Description :** étend le gabarit §8 avec ce que `writing-plans` apporte de réellement
utile ici :

- **L'en-tête** : Objectif / Architecture / Spec, puis **Contraintes globales** (les
  exigences valables pour toutes les étapes, une ligne chacune) et **Review Focus** (les
  modes de défaillance que les tests du plan n'exercent pas et qui mordraient un
  utilisateur — écrits une fois, au moment du plan, et relus à la clôture).
- **Le bloc Consomme / Produit par étape.** C'est la garde qui manque le plus à rompom :
  `CLAUDE.md` établit que chaque step alimente les suivants **par le `Rom`, pas par le
  disque**, et un plan qui ajoute un step devrait déclarer ce qu'il y dépose et ce qu'il
  y lit. « L'étape 2 remplit `rom.medias`, l'étape 3 le consomme » devient une dépendance
  déclarée plutôt qu'une chose à se rappeler.
- **La règle « pas de réservé »** : ni « TBD », ni « à compléter », ni « gérer les cas
  limites » — une étape qui décrit quoi faire sans montrer comment est un défaut de plan.
- **Un balayage pré-vol** avant la première étape : pour chaque étape qui consomme ce
  qu'une précédente produit, une ligne de journal disant ce qu'on a trouvé.

Ce qui n'est **pas** repris : l'emplacement `docs/superpowers/plans/`, et la granularité
« une étape = 2 à 5 minutes = un commit », qui produirait des commits rouges (étape 4).

**Produit :** le gabarit étendu.
**Consomme :** de l'étape 1, le fait que seul un changement architectural ait un plan ;
de l'étape 5, le chemin du journal.

**Fichiers :** `PROCEDURE_PLANS.md` (§8).

**Vérification :** **ce fichier de plan satisfait le gabarit étendu** — en-tête,
Contraintes globales, Review Focus, Consomme/Produit par étape, aucun réservé.

**Commit :** `docs(procedure): extend the plan template with constraints and interfaces`

---

### Étape 7 : Table d'arbitrage des skills

**Description :** une ligne par skill `superpowers`, avec son verdict et sa
justification, dans `PROCEDURE_PLANS.md` ; la table condensée dans `CLAUDE.md`. La
section s'ouvre sur la phrase qui fait le travail : **là où un skill et ce document ne
disent pas la même chose, ce document l'emporte** — arbitré contre `superpowers` 6.4.1.
Une légende définit les huit verdicts (*Adopté*, *Adapté*, *Réduit*, *Remplacé*, *Rejeté
par défaut*, *Rejeté*, *Surclassé*, *Marginal*), sans quoi les mots ne contraignent rien.

Les 15 verdicts, tels qu'arrêtés à l'étude :

| Skill | Verdict |
|---|---|
| `systematic-debugging` | Adopté (étape 3) |
| `verification-before-completion` | Adopté (étape 3) |
| `receiving-code-review` | Adopté (étape 5) |
| `finishing-a-development-branch` | Adopté, sous la règle du lot (étape 2) |
| `test-driven-development` | Adapté (étape 4) |
| `writing-plans` | Adapté (étape 6) |
| `executing-plans` | Adapté — journal commité à côté du plan, pas d'espace `.superpowers/sdd/`, et le journal ne se supprime pas en fin de chantier (étape 5) |
| `requesting-code-review` | Adapté — **une** relecture à la clôture, pas une par tâche (étape 5) |
| `brainstorming` | Réduit — le triage seul, plus *Trivial* ; pas de spec séparée, pas de compagnon visuel (étape 1) |
| `subagent-driven-development` | Rejeté par défaut — à proposer au-delà de ~8 étapes |
| `using-git-worktrees` | Rejeté — `target/` non partagé, `.direnv` perdu, et son étape 2 lance `cargo build` hors `nix develop` ; la branche dédiée est l'isolation. Rejet de son workflow manuel, pas d'un worktree natif demandé |
| `using-superpowers` | Surclassé — son « 1 % » ne bat pas une ligne de cette table |
| `dispatching-parallel-agents` | Marginal |
| `writing-skills` | Marginal — rompom n'écrit aucun skill ; aucune porte du dépôt ne peut le vérifier |
| `diagnosing-superpowers` | Marginal — rapport de bug au plugin, sans rapport avec rompom |

`CLAUDE.md` porte la même table, groupée par verdict, et **renvoie à
`PROCEDURE_PLANS.md` comme autorité** : les deux peuvent diverger, une divergence se
tranche en faveur du second.

**Produit :** la section `§ Arbitrage des skills`.
**Consomme :** les titres de section produits aux étapes 1, 3, 5 et 6, cités dans la
colonne de justification.

**Fichiers :** `PROCEDURE_PLANS.md`, `CLAUDE.md`.

**Vérification :** `ls ~/.claude/plugins/cache/claude-plugins-official/superpowers/6.4.1/skills/
| wc -l` → 15, et **15 lignes dans chacune des deux tables** ; chaque renvoi de section
pointe sur un titre qui existe ; la légende définit tous les verdicts employés.

**Commit :** `docs(procedure): arbitrate the superpowers skills`

---

### Étape 8 : Retirer OpenSpec

**Description :** `openspec/`, `.claude/commands/opsx/` et les six
`.claude/skills/openspec-*` sont installés mais n'ont jamais servi : `config.yaml` est le
fichier par défaut sans une ligne de contexte projet, `specs/` et `changes/archive/` ne
contiennent que des `.gitkeep`, et rien n'est suivi par git. Ils doublent
`brainstorming` / `writing-plans` / `executing-plans` avec des artefacts plus lourds,
alors que rompom a une convention de plans qui tourne depuis v0.16.

Suppression des trois emplacements. Comme rien n'est suivi par git, **la suppression ne
produit aucun diff** — d'où la seconde moitié de l'étape, qui est la seule à laisser une
trace : **une ligne dans la section d'arbitrage disant qu'OpenSpec n'est pas le processus
de rompom**, pour qu'une session future ne le réinstalle pas.

À noter, et à écrire : les skills `opsx:*` fournis par un **plugin** restent listés à
chaque session quoi qu'on supprime localement. Retirer l'échafaudage ne les délie pas —
c'est la ligne d'arbitrage qui le fait.

**Produit :** la ligne d'arbitrage OpenSpec.
**Consomme :** de l'étape 7, la section `§ Arbitrage des skills` où la ligne s'insère.

**Fichiers :** `PROCEDURE_PLANS.md` ; suppressions hors diff.

**Vérification :** `ls openspec .claude/commands/opsx .claude/skills` → les trois chemins
OpenSpec ont disparu ; `git status --porcelain` ne montre plus les trois entrées `??` ;
la ligne d'arbitrage est présente et nomme les skills `opsx:*` du plugin.

**Commit :** `docs(procedure): record that OpenSpec is not rompom's planning process`

---

## Portes de qualité

- [ ] `just ci` passe (aucun code touché, mais la porte reste la porte)
- [ ] Aucun fichier `src/`, `assets/` ou `.nix` dans le diff
- [ ] Les 15 skills figurent dans **les deux** tables, `PROCEDURE_PLANS.md` et `CLAUDE.md`
- [ ] Aucune règle normative survivante disant que Claude ne merge/push/tag pas — grep
      écrit en français
- [ ] Chaque renvoi de section pointe sur un titre qui existe
- [ ] Ce fichier de plan satisfait le gabarit étendu de l'étape 6
- [ ] Commits atomiques, un par ligne de changelog voulue, sujets rédigés comme des notes
      de version
- [ ] Journal tenu dans le même appel que chaque commit
- [ ] Relecture de clôture par un contexte frais, ses constats regradés par effet
