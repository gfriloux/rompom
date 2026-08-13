# Phase 0 — audit du dépôt avant de coder

**Date :** 2026-08-13
**Commit de départ :** `0c99661` (merge de `refactor/v0.21.0`), `master`, working tree propre.
**Version courante :** `0.21.0`

## `nix develop --command just ci`

```
version-check  ✓   Cargo.toml == Cargo.lock == packages/rompom/default.nix
fmt-check      ✓
lint           ✓   clippy -D warnings, aucun warning
test           ✓   185 passed; 0 failed; 0 ignored
```

Sortie : **exit 0**.

## Ce que l'audit a confirmé dans le code (lu, pas supposé)

- `finish_rom()` (`worker/mod.rs:306`) décrémente `remaining` **une fois par ROM**, sur la
  feuille, gardé par `Rom::finished`. C'est l'invariant que la relance doit défaire
  proprement : re-armer une ROM veut dire `finished = false` **et** `remaining += 1`.
- `TaskQueue::shutdown()` (`queue.rs:250`) pose `inner.shutdown = true` et rien ne le
  remet à `false` — la queue est à sens unique aujourd'hui.
- `worker_loop_main` / `worker_loop_blocking` ne font `ss_sem.cancel()` **que** si
  `interrupted`. À une fin de run normale le sémaphore ressort intact, tous ses permits
  rendus : il est réutilisable pour un second tour.
- Les candidats du modal vivent dans `pipeline[LookupSS].data`
  (`handlers/discovery.rs:269-272`), lus par `WaitModal` par clone
  (`discovery.rs:314`). Un `WaitModal` échoué peut donc être re-armé seul : ses candidats
  sont toujours là.
- `Rate::tick` (`ui/rate.rs:60`) fait `done.saturating_sub(self.last_done)` — un compteur
  `done` qui **redescend** (ce que fait la relance en effaçant un `finished_at`) ne casse
  ni la sparkline ni l'ETA.
- Une identification refusée par l'utilisateur (`ModalResponse::Cancelled`) ne produit
  **pas** de step `Failed` : `bar.not_found()` rougit la cellule `id` mais la ROM finit
  par `finish()`, donc `RomEntry::error` reste `None`. Elle n'est ni dans la vue erreurs
  ni relançable par `r` — voir « hors périmètre » du plan.
