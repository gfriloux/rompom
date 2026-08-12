# Phase 0 — audit avant de coder (v0.21.0)

Lancé le 2026-08-12 sur `master` à `29b3e55`, working tree propre.

```
nix develop --command just ci
```

| porte | résultat |
|---|---|
| `version-check` | ok — `0.20.1` dans `Cargo.toml`, `Cargo.lock`, `packages/rompom/default.nix` |
| `fmt-check` | ok |
| `lint` (clippy `-D warnings`) | ok |
| `test` | **160 passés**, 0 échec, 0 ignoré |

Sortie : code 0.

Rien à nettoyer avant de commencer : le lot part d'un dépôt vert.

## Relevé de départ

Ce que les étapes vont modifier, mesuré avant :

- 160 tests
- `src/package.rs` : 1027 lignes, dont 62 pour les huit blocs de `build_pkgbuild`
- `src/worker/handlers/discovery.rs` : 462 lignes, dont 2 × 39 pour `rom_unchanged`
- `src/worker/handlers/downloads.rs` : 300 lignes
- `src/queue.rs` : 168 lignes
- Liste des huit médias écrite en 5 endroits (+ `MEDIA_ICONS` pour l'ordre d'affichage),
  selon **deux** ordres différents
