# Journal — plan : .claude/plans/process/superpowers-arbitrage/plan.md

## Balayage pré-vol

Une ligne par étape qui consomme ce qu'une précédente produit.

| Consommateur | Producteur | Ce qui est consommé | Trouvé |
|---|---|---|---|
| Étape 5 | Étape 1 | l'arborescence `.claude/plans/process/<nom>/` | les noms concordent |
| Étape 5 | Étape 2 | la « règle du lot », citée parmi les cinq arrêts | les noms concordent |
| Étape 5 | Étape 3 | l'arrêt à trois correctifs échoués, cinquième arrêt | **manquait au bloc Consomme de l'étape 5** — voir décision ci-dessous |
| Étape 6 | Étape 1 | « seul un changement architectural a un plan » | concorde |
| Étape 6 | Étape 5 | le chemin `progress.md` | concorde |
| Étape 7 | Étapes 1, 3, 5, 6 | les titres de section cités en colonne de justification | les quatre sont produits avant l'étape 7 |
| Étape 8 | Étape 7 | la section `§ Arbitrage des skills` où la ligne s'insère | concorde |

Les étapes 2, 3 et 4 ne consomment rien de ce plan.

Décision : le bloc *Consomme* de l'étape 5 nomme les étapes 1 et 2 mais pas l'étape 3,
alors que sa ligne de vérification exige que l'arrêt à trois correctifs figure parmi les
cinq — l'étape 3 est donc bien un producteur pour l'étape 5. Je traite la dépendance
comme déclarée et ne réécris pas le plan pour si peu. — Pourquoi : le contenu est déjà
juste, seule la déclaration était incomplète, et la vérification de l'étape 5 attrape la
faute que la déclaration aurait prévenue. — Coût si c'est faux : l'étape 3 est écrite sans
l'arrêt à trois correctifs et l'étape 5 cite quatre arrêts en en annonçant cinq, ce que sa
propre vérification refuse.

C'est le balayage pré-vol qui a trouvé ça, sur le premier plan écrit au gabarit étendu —
une confirmation que le bloc *Consomme/Produit* de l'étape 6 gagne sa place.

## Étapes

Étape 0 : terminée. Branche `docs/superpowers-arbitrage` créée depuis `master` à `2a07b0d`.
`nix develop --command just ci` → `EXIT=0`, quatre portes, 192 tests, 0 échec. Consigné
dans `phase0_results.md`.

Étape 1 : terminée (`1e6db0d`, signée `G`). §2 à §8 gardent leurs numéros ; les quatre
chemins sont présents ; la seule occurrence survivante de « sans plan validé » est
restreinte au chemin architectural.

Décision : la section de dimensionnement ouvre §1 au lieu de devenir une section neuve, et
les sections suivantes ne sont pas renumérotées. — Pourquoi : `git grep '§[0-9]'` montre que
les plans archivés sous `.claude/plans/v*/` renvoient à §1, §3, §4 et §5 par leur numéro, et
la contrainte globale interdit de réécrire ces archives ; renuméroter falsifierait
silencieusement des renvois justes. — Coût si c'est faux : le dimensionnement se lit comme un
préambule au plan plutôt que comme une porte autonome, ce que le titre de §1 compense.

Décision : l'étape 1 touche aussi `CLAUDE.md`, que son bloc *Fichiers* ne prévoyait pas. —
Pourquoi : « No coding without a validated plan » y figurait deux fois, et `CLAUDE.md` est le
seul fichier réinjecté à chaque session ; le laisser intact aurait défait §1 depuis le seul
endroit toujours en contexte. C'est le constat Critique que la relecture de clôture du dépôt
voisin a levé sur son propre chantier — le reproduire en le sachant n'était pas tenable. —
Coût si c'est faux : deux lignes de `CLAUDE.md` changent un commit plus tôt que prévu.

Note : les hooks pre-commit ne se déclenchent que sur `.nix`, `.rs` et les fichiers de
version. Aucune porte ne tourne sur un commit de `.md` — c'est le `just ci` de la phase 0 qui
fait foi pour tout ce chantier, et il n'y a rien d'autre à prouver puisque aucun code n'est
touché.

Étape 2 : terminée (`5e12aac`, signée `G`). Plus aucune règle normative ne dit que Claude ne
merge/push/tag pas, dans les deux langues ; les seules occurrences restantes de
« par l'utilisateur » concernent la relecture du plan, pas git.

Décision : la ligne `- [ ] Branche dédiée, non mergée par Claude` du gabarit §8 est corrigée
à l'étape 2 et non à l'étape 6, qui est celle du gabarit. — Pourquoi : elle contredisait le
§3 fraîchement écrit, et la laisser debout quatre commits aurait fait cohabiter deux normes
git dans le même fichier. — Coût si c'est faux : une ligne du gabarit change un commit plus
tôt que prévu, sans effet sur l'étape 6.

Note de vérification, et c'est un constat sur la vérification elle-même : le grep annoncé par
le plan (`jamais.*(merge|push|tag)|ne fait jamais`) **a raté** cette ligne, parce qu'elle
écrit le concept autrement (« non mergée par Claude »). Un grep bâti sur la formulation qu'on
attend ne prouve pas l'absence du concept ; il a fallu chercher `merg` et lire les six
résultats. Même classe d'erreur que le grep anglais-seulement relevé par la relecture du
dépôt voisin — ici ce n'était pas la langue, c'était la tournure.

Étape 3 : terminée (`a0535ad`, signée `G`). Les dix sections existent, les sept renvois `§N`
du dépôt pointent tous sur un titre présent, et les trois recettes citées dans la table de
preuves (`ci`, `test`, `build-static`) existent bien dans le `Justfile`.

Décision : la section s'insère en **§8**, ce qui décale Gabarit en §9 et « Ce qui ne change
pas » en §10, plutôt que d'être appendue en §10 après le récapitulatif de clôture. — Pourquoi :
`git grep` ne montre aucun renvoi externe à §8, et le seul renvoi à §9 est
`design/tui/README.md`, une documentation vivante et non l'archive d'un chantier clos — donc
corrigeable dans le même commit, ce que la règle « la doc part avec le code » demande de toute
façon. §1 à §7 gardent leurs numéros, qui sont ceux que les archives citent. — Coût si c'est
faux : un renvoi à §9/§10 dort dans un fichier non suivi par git que le grep ne voit pas.

Décision : l'intro de §8 n'annonce pas de décompte de règles (« cinq règles ») alors que deux
seulement sont écrites à cette étape ; c'est l'étape 5, qui ferme la section, qui fixera le
compte. — Pourquoi : la relecture de clôture du dépôt voisin a relevé exactement ça chez lui,
un intitulé annonçant quatre règles au-dessus d'une section qui en portait cinq. Un commit qui
promet ce qu'il ne contient pas encore est faux pendant deux commits. — Coût si c'est faux :
l'intro est plus vague qu'elle pourrait l'être pendant deux commits.

Étape 4 : terminée (`4635518`, signée `G`). §4 et §5 disent la même chose sur le commit unique
test + correctif.

Décision : la liste des exclusions ajoute « les handlers qui en dépendent », que §4 ne nommait
pas, alors que la vérification du plan interdisait d'ajouter ou de retirer un élément au
passage. — Pourquoi : c'est descriptif, pas normatif neuf. `CLAUDE.md` l'écrit déjà noir sur
blanc pour `BuildPackage` et `DownloadMedias` (« There is no test: both handlers need a
ScreenScraper client and a network »). Ne pas les nommer laissait lire « l'appel n'est pas
testé, mais le handler devrait l'être ». — Coût si c'est faux : un handler purifiable échappe
à la loi des tests en s'abritant derrière cette ligne ; la parade est que §4 ne parle que des
handlers *qui dépendent* du réseau.

Décision : ajout d'une cinquième règle non prévue au plan, « un refactor ne change aucun
test ». — Pourquoi : §5 dit déjà qu'un snapshot qui bouge disqualifie un refactor, et la même
chose vaut pour un test ; la généralisation tient en deux lignes au bon endroit. — Coût si
c'est faux : deux lignes à retirer, sans effet ailleurs.

Étape 5 : terminée (`05a3050`, signée `G`). §8 annonce quatre règles et porte exactement
quatre sous-sections ; les cinq arrêts sont au nombre de cinq et incluent celui à trois
correctifs échoués introduit à l'étape 3 ; le chemin `progress.md` est identique en §1 et §8 ;
la dérogation sous-agent est nommée comme dérogation.

Décision : `design/diagrams/` est non suivi par git et préexiste au chantier. Je le laisse
intact et le consigne en phase 0 plutôt que de le traiter en passant. — Pourquoi : il ne
relève pas de l'arbitrage superpowers, et décider du sort d'un répertoire qu'un autre
chantier a laissé là n'est pas à moi de le faire au détour de celui-ci. — Coût si c'est
faux : il reste `??` dans `git status` une release de plus.
