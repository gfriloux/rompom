# Phase 0 — Audit de l'état réel avant v0.16.0

**Date :** 2026-08-09
**HEAD :** `509d935` (master, à jour avec `origin/master`)
**Commande :** `nix develop --command just ci`

## Résultat : vert

| Porte | Résultat |
|---|---|
| `version-check` | OK — `Cargo.toml` = `Cargo.lock` = `packages/rompom/default.nix` = `0.15.0` |
| `fmt-check` | OK |
| `lint` (clippy `-D warnings`) | OK |
| `test` | OK — **2 tests**, 0 échec |
| `nix flake check` | OK — alejandra, deadnix, statix, rustfmt |
| `just audit` | OK — 0 vulnérabilité, **9 avertissements** *unmaintained/unsound* tolérés |

CI GitHub verte sur `master` (confirmée par l'utilisateur).

## Working tree

Propre, à trois exceptions **non suivies** et volontairement laissées de côté :
`PLAN_DOCUMENTATION.md`, `PLAN_SS_ROM_CONTRIBUTION.md`, `PLAN_STATE_MACHINE.md`.
`PROCEDURE_PLANS.md` §1 les cite comme faisant partie de la roadmap : décider de les
committer ou de retirer la référence.

## Constats à porter dans le plan

1. **Aucun test sur le code visé par P0.** Les 2 tests existants couvrent
   `generate_description_xml()`. Tout le périmètre P0 (échappement, traversal, DAG,
   resume, sorties d'erreur) est à découvert. Conséquence directe : chaque correction P0
   commence par un test qui échoue (`PROCEDURE_PLANS.md` §5).

2. **`normalize_name()` est une liste noire** (`package.rs:97-114`) : elle retire
   `( ) espace , ' ! & % ^ ; $ ~` mais **ni `"`, ni backtick, ni `\`, ni `\n`**. Une liste
   noire ne peut pas être exhaustive — P0.1 doit passer à une liste blanche.

3. **`pkgdesc` n'est pas normalisé du tout** : le nom de jeu ScreenScraper est injecté
   brut dans le PKGBUILD, et MiniJinja est enregistré sous le nom `"t"`
   (`package.rs:55`), ce qui désactive l'auto-échappement.

4. **`execute_step` n'a pas de `catch_unwind`** (`worker/mod.rs:84-167`) et appelle
   `do_dispatch` en ligne 166 **y compris après un `Failed` définitif** (ligne 153).
   Les deux défauts sont dans la même fonction : P0.3 et P0.4 se traitent ensemble.

5. **`.ok()` sur les appels SS** (`discovery.rs:250`, `:255`) confond erreur réseau et
   jeu introuvable — c'est P1.1, et ça touche le même code que P0.5 (resume/LookupSS).
   Les traiter dans la même version évite de repasser deux fois au même endroit.

6. **`main.rs` compte ~20 `unwrap()`**, dont ceux de la collecte visés par P0.6
   (`:395` `Metadata::get`, `:402`/`:450` `Pattern::new`, `:440` `read_dir`, `:509`
   `ScreenScraper::new`). Les `lock().unwrap()` sur mutex ne sont **pas** concernés :
   un mutex empoisonné est déjà un bug fatal ailleurs.

7. **Renovate est actif** et a ouvert deux PR dès l'installation :
   - #19 `chore(deps): Update Rust crate openssl to v0.10.80` **[SECURITY]**
   - #20 `chore(deps): Update Rust crate chrono to v0.4.45`

   La #19 est à merger **avant** de commencer : c'est une correction de sécurité, elle
   entre naturellement dans le périmètre v0.16.0, et `cargo audit` ne l'avait pas
   remontée (base d'advisories locale plus ancienne que celle de Renovate).

8. **Bruit de build connu** : `unused manifest key: target.x86_64-unknown-linux-gnu.rustflags`
   à chaque `cargo` — c'est P1.4, hors périmètre de ce plan, mais trivial à supprimer si
   on veut un log propre.
