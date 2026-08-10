# Phase 0 — audit du dépôt avant de coder (v0.17.0)

Exécuté le 2026-08-10 sur `master`, commit `e07b0e2` (`chore(release): v0.16.0`).

## `nix develop --command just ci`

**Exit code 0.** Les quatre portes passent : `version-check`, `fmt-check`,
`clippy -D warnings`, `test`.

- **34 tests**, 0 échec, 0 ignoré (`emulationstation` 4, `package` 9, `collect`/`tests` 3,
  `worker::helpers` 3, `worker::run_state` 5, `worker` 10).

## État du working tree

Propre côté code. Deux modifications de documentation non commitées, faites le même jour :

- `PROCEDURE_PLANS.md` — §1 ne référence plus les plans de cadrage supprimés.
- `TODO.md` — renvois aux plans supprimés retirés ; le blocage de la contribution SS
  (endpoint non documenté) est replié dans P3.

Les trois plans historiques de la racine (`PLAN_DOCUMENTATION.md`,
`PLAN_SS_ROM_CONTRIBUTION.md`, `PLAN_STATE_MACHINE.md`) ont été **supprimés** : les deux
premiers étaient réalisés (README actuel, DAG en place), le troisième était bloqué et son
contenu utile vit désormais dans `TODO.md`.

## Dépendance `screenscraper`

`Cargo.toml` pointe encore `tag = "v0.6.0"`. La **v0.7.0 est taguée** en amont et contient
ce que P1.1 attend — vérifié dans `../screenscraper` :

- `api::Error::Http { url, status, body }` — `get()` lit `response.status()` avant
  `.text()`, le `url` reste l'endpoint nu (pas de fuite de `devpassword`/`sspassword`),
  le corps est tronqué.
- `pub enum ApiFailure` — 13 variantes, dont `NotFound`, `ThreadLimit`, `QuotaExceeded`,
  `Transport`, `Malformed`, `Api`.
- `Error::failure()`, `Error::is_retryable()`, `Error::is_not_found()` sur l'erreur
  **publique**, qui traversent les quatre wrappers.
- `classify(status)` pure, testée code par code.

`is_retryable()` est vrai pour `ThreadLimit` (429), `ServerBusy` (401) et `Transport`
uniquement — conforme à ce que le handoff demandait.

## Ce qui reste à vérifier au moment du bump

`packages/rompom/default.nix` fige `"screenscraper-0.6.0" = "sha256-FIFSnDOjIycPYLgRkyfA0OY8jCIV9/W5kVto0+zeMAk="`.
La clé **et** le hash changent en 0.7.0 ; à récupérer via `nix build` (`got:`).
