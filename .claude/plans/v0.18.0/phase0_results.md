# Phase 0 — audit du dépôt avant de coder (v0.18.0)

Exécuté le 2026-08-10, avant toute modification.

## Point de départ

- Branche : `master` @ `20621b4` (merge de `feat/v0.17.0`)
- Working tree : **propre** (`git status --short` ne rend rien)
- Version : `0.17.0` partout (Cargo.toml / Cargo.lock / packages/rompom/default.nix)

## `nix develop --command just ci`

**Exit 0.** Les quatre portes passent :

| porte | résultat |
|---|---|
| `version-check` | ok — 0.17.0 == 0.17.0 == 0.17.0 |
| `fmt-check` | ok |
| `lint` (clippy `-D warnings`) | ok |
| `test` | **59 passed, 0 failed** |

## `nix develop --command just audit`

**0 vulnérabilité**, 9 avertissements tous couverts par `.cargo/audit.toml` :
`RUSTSEC-2022-0004` (rustc-serialize ← shaman ← checksums) et les trois
`RUSTSEC-2026-0098/0099/0104` (rustls-webpki ← rustls 0.21 ← reqwest 0.11) sont
ignorés avec justification ; le reste est *unmaintained/unsound* non bloquant
(`shaman`, `atty`, `ansi_term`, `paste`, `lru`, `anyhow`…).

## Constats qui cadrent le plan

1. **`TODO.md` est en retard sur le dépôt pour P1.6.** Il réclame encore le
   round-trip `SystemState` et `apply_run_state()` : les deux existent déjà
   (`state::tests::a_state_survives_a_round_trip`, et sept tests dans
   `worker::run_state::tests`). Restent réellement à écrire : `disc_indicator()`,
   `group_multi_disc()`, `search_name()`, `check_media_changes()`,
   `apply_game_path()`, `read_pkgver()`.

2. **Les 10 dépendances `*` résolvent aujourd'hui vers** (relevé dans `Cargo.lock`) :
   `checksums 0.9.1`, `chrono 0.4.45`, `dirs 6.0.0`, `getopts 0.2.24`,
   `quick-xml 0.41.0`, `serde_derive 1.0.228`, `serde_json 1.0.149`,
   `serde_yaml 0.9.34+deprecated`, `snafu 0.7.5`, `reqwest 0.11.27`,
   `openssl 0.10.80`.

3. **`serde_derive` est importé dans deux fichiers** (`src/conf/mod.rs:3`,
   `src/emulationstation.rs:2`). Le retirer n'est pas qu'une ligne de `Cargo.toml` :
   il faut basculer les deux `use` sur `serde::{Deserialize, Serialize}` (la feature
   `derive` de `serde` est déjà activée).

4. **Le profil release vit aujourd'hui uniquement dans le Nix** (`env = { CARGO_PROFILE_RELEASE_* }`),
   avec un commentaire de 12 lignes expliquant pourquoi `panic = "abort"` est
   volontairement absent, et le coût mesuré (+483 Ko / +4,3 %). Ce commentaire doit
   suivre le profil s'il déménage dans `Cargo.toml`, sinon le prochain qui « optimise
   la taille » rend `catch_unwind` mort dans le binaire livré.

5. **Aucune détection de tty** : `Ui::new()` fait `enable_raw_mode().unwrap()` puis
   `EnterAlternateScreen` inconditionnellement (`src/ui/mod.rs:365-369`).

6. **Le handoff design « turn 4 » est toujours dans `tmp/`** — non versionné, et `tmp/`
   est du scratch par convention. Deux fichiers : `README.md`, `mockups.dc.html`.
