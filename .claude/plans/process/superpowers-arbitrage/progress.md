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

Décision : `design/diagrams/` est non suivi par git et préexiste au chantier. Je le laisse
intact et le consigne en phase 0 plutôt que de le traiter en passant. — Pourquoi : il ne
relève pas de l'arbitrage superpowers, et décider du sort d'un répertoire qu'un autre
chantier a laissé là n'est pas à moi de le faire au détour de celui-ci. — Coût si c'est
faux : il reste `??` dans `git status` une release de plus.
