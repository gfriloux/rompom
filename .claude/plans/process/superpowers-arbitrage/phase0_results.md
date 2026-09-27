# Phase 0 — état du dépôt avant le chantier

**Date :** 2026-09-27
**Branche :** `docs/superpowers-arbitrage`, créée depuis `master` à `2a07b0d`
(`chore(release): v0.22.0`)

## `just ci`

```
nix develop --command just ci
EXIT=0
```

Les quatre portes ont tourné dans l'ordre de la recette (`ci: version-check fmt-check
lint test`) :

| Porte | Sortie |
|---|---|
| `version-check` | `Cargo.toml=0.22.0  Cargo.lock=0.22.0  packages/rompom/default.nix=0.22.0` |
| `fmt-check` | `cargo fmt --check -- --config tab_spaces=2` — aucune sortie |
| `lint` | `cargo clippy --all-targets -- -D warnings` — `Finished dev profile`, aucun avertissement |
| `test` | `test result: ok. 192 passed; 0 failed; 0 ignored` |

Note de méthode : les deux premières tentatives ont affiché un code de sortie **vide**,
`${pipestatus[1]}` puis `$status` ayant été écrits en syntaxe fish alors que l'outil
exécute `bash`. Le vert n'a été retenu qu'à la troisième, avec `$?`. C'est précisément le
faux vert que la table de preuves de l'étape 3 existe pour attraper : la queue de
`just test` était verte les trois fois, et ne disait rien des quatre portes.

## Working tree

```
?? .claude/commands/
?? .claude/plans/process/
?? .claude/skills/
?? design/diagrams/
?? openspec/
```

Trois de ces entrées disparaissent à l'étape 8 (`openspec/`, `.claude/commands/opsx/`,
`.claude/skills/openspec-*`). `.claude/plans/process/` est ce chantier, commité à
l'étape 1.

**`design/diagrams/` préexiste et sort du périmètre.** Il n'a pas été créé par ce
chantier et n'est pas touché par lui ; il est consigné ici pour qu'un lecteur ultérieur
ne le mette pas sur notre compte. Son sort est une décision distincte.

## Ce que cette phase 0 ne prouve pas

`just ci` compile en **debug**. Le paquet Nix build en release, et ce chantier ne touche
aucun code — donc `just build-static` ne sera pas relancé. La porte de release reste
celle du mainteneur au moment du tag.
