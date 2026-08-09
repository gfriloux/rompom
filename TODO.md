# TODO — Roadmap qualité & simplicité

Issu de la revue complète du 2026-08-08 (sécurité, concurrence, UX/CLI, outillage).

Diagnostic transverse : le chemin nominal est soigné, mais les chemins d'échec
(panique, step Failed, resume partiel, données réseau hostiles) ne sont pas traités
comme des chemins de première classe. C'est là que se concentrent les problèmes graves.

Points sains à préserver : architecture DAG + TaskQueue, extraction sous lock avant
I/O réseau, write-rename du state.yml, code clippy-clean, Cargo.lock commité,
TLS correct, pas de fuite de credentials, XML échappé via quick-xml.

---

## P0 — Corruption / sécurité (avant toute feature)

- [ ] **P0.1 — Injection shell dans les PKGBUILD** (HAUTE). Les templates MiniJinja sont
  enregistrés sous le nom `"t"` (`package.rs:55`) → auto-échappement désactivé.
  `pkgdesc` (nom de jeu SS) et `_romname` sont injectés bruts : un nom contenant
  `"$(...)"` = RCE au `makepkg` sur la Batocera. `normalize_name()` ne retire ni `"`,
  ni backtick, ni `\`, ni retour à la ligne.
  → Fonction d'échappement shell unique appliquée à tout champ SS/IA injecté dans un
  PKGBUILD (`pkgdesc`, `romname`, `pkgname`) + whitelist alphanumérique pour
  `format`/`region` des médias + validation `sha1 =~ ^[0-9a-f]{40}$`. *(petit)*
- [ ] **P0.2 — Path traversal via `m.format`** : `media_filename()` (`worker/helpers.rs:26`)
  accepte `png/../../../x` → écriture hors du répertoire de sortie
  (`downloads.rs:231`). Rejeter `/`, `\`, `..`. *(petit)*
- [ ] **P0.3 — Panique de handler = deadlock global** : pas de `catch_unwind` dans
  `execute_step` (`worker/mod.rs:84-167`). Un handler qui panique (date SS malformée
  `emulationstation.rs:73`, `hash_file` sur fichier disparu) tue le worker,
  `remaining` n'est jamais décrémenté, tout bloque. → `catch_unwind` → `Failed`. *(petit)*
- [ ] **P0.4 — Step `Failed` corrompt l'état** : `do_dispatch` est appelé même après
  échec définitif (`worker/mod.rs:166`) → `SaveState` persiste le sha1 attendu d'une
  ROM jamais téléchargée (plus jamais retentée) + double comptage UI (1 erreur +
  1 succès). → Marquer les successeurs Skipped/Failed, ne pas persister, ne pas
  re-finir la barre. *(moyen)*
- [ ] **P0.5 — Resume corrompt les paquets** : `run.yml` ne stocke que les statuts ;
  `jeu`/`medias` ne sont pas re-dérivés. Interruption entre LookupSS (Done) et
  BuildPackage (Pending) → description.xml vide écrase le bon + bump pkgver ;
  SaveState peut écraser l'état avec map vide (perte du cache ss_game_id).
  → Au resume, si BuildPackage/SaveState Pending, remettre LookupSS à Pending. *(petit/moyen)*
- [ ] **P0.6 — `unwrap()` de la collecte paniquent sous TUI** (erreur invisible,
  terminal corrompu) : `Metadata::get` (`main.rs:395`), `ScreenScraper::new`
  (`main.rs:509`, credentials faux !), `Pattern::new` (`main.rs:402,450`, glob invalide
  dans la config), `read_dir` (`main.rs:440`), `parse_from_str` sur date SS
  (`emulationstation.rs:73`). → Sortir du TUI proprement puis eprintln + exit. *(moyen)*

## P1 — Robustesse et confiance

- [ ] **P1.1 — Erreur réseau SS ≠ « jeu non trouvé »** : `jeuinfo(...).ok()`
  (`discovery.rs:249-256`) avale timeout/500/quota → modale d'identification
  injustifiée ; le retry de LookupSS est du code mort. → Distinguer `Err` (retry)
  de `Ok(None)` (modale). *(petit)*
- [ ] **P1.2 — Afficher les erreurs des ROMs échouées** : `StepStatus::Failed(msg)`
  existe mais `finish_error()` ne prend pas le message ; le summary n'imprime qu'un
  compteur. → Panneau Completed `✗ rom — cause` + liste des échecs dans
  `Summary::print()`. *(petit/moyen)*
- [ ] **P1.3 — Messages d'erreur config** : `ReadConfiguration`/`ParseConfiguration`
  sans `#[snafu(display)]` (`conf/mod.rs:100-113`) → l'erreur serde_yaml
  (ligne/colonne) et le chemin sont perdus. *(petit)*
- [ ] **P1.4 — Cargo.toml** : pinner les 10 dépendances `*` (valeurs du lock, reqwest
  est à 0.11.27), supprimer la section `[target.x86_64...]` invalide (ignorée par
  cargo), ajouter `[profile.release]` (opt-level=z, LTO — aujourd'hui seulement dans
  le Nix), retirer `serde_derive` redondant. *(petit)*
- [x] **P1.5 — CI GitHub Actions** — *fait le 2026-08-09*. `.github/workflows/ci.yml` :
  job `ci` (`just ci` = version-check + fmt-check + clippy `-D warnings` + test),
  job `nix` (`nix flake check`), job `audit` (`cargo audit`, advisory). Le `Justfile` est
  la seule définition des portes ; pre-commit l'appelle aussi. Voir `PROCEDURE_PLANS.md` §7.
- [ ] **P1.6 — Premiers tests unitaires** (fonctions pures, sans réseau) :
  `disc_indicator()`, `group_multi_disc()`, `search_name()`, `check_media_changes()`,
  snapshot XML de `generate_description_xml()` + `apply_game_path()`,
  `read_pkgver()` + round-trip `SystemState`. Puis `apply_run_state()`
  (invariant anti-underflow). Extraire `disc_indicator`/`group_multi_disc` vers
  `src/collect.rs` au passage. *(petit chacun)*
- [ ] **P1.7 — `./launcher` OpenBOR écrit en CWD** (`package.rs:181`) : chemin partagé
  entre workers concurrents → l'écrire dans le répertoire de la ROM. *(petit)*
- [ ] **P1.8 — Durabilité de l'état** : flush périodique du state (un crash ≠ Ctrl-C
  perd tout → re-bump de tous les pkgver) ; write-rename pour run.yml ; warning si
  state.yml existe mais est illisible (aujourd'hui ignoré en silence,
  `state.rs:29-31`). *(petit)*

## P2 — Simplicité d'utilisation

- [ ] **P2.1 — CLI** : supprimer le `panic!` sur argument invalide (`main.rs:264-267`),
  ajouter `--version`, exit codes cohérents (système inconnu / sans source sortent
  en 0 aujourd'hui), `--list-systems`. *(petit)*
- [ ] **P2.2 — `rompom --init`** : écrire un template commenté dans
  `~/.config/rompom.yml` s'il n'existe pas (le sample de la racine via
  `include_str!`) ; valider `lang` contre `SUPPORTED_LANGS` au chargement. *(petit/moyen)*
- [ ] **P2.3 — Statut `retrying (2/3)...`** sur la barre pendant les retries
  (aujourd'hui invisibles). *(petit)*
- [ ] **P2.4 — Multi-disc, cas limites** : tags région après l'indicateur perdus/fusionnés
  (`Game (Disc 1) (USA)` + `Game (Disc 2) (Europe)` fusionnent) ; numéros de disque
  dupliqués acceptés → m3u faux ; `(CD32)` matché comme disque 32. *(moyen)*
- [ ] **P2.5 — Documentation** : Nerd Fonts requis (+ fallback `--ascii`, les icônes
  sont centralisées dans `MEDIA_ICONS`), fichiers créés dans le cwd
  (state.yml/run.yml/debug.log), conséquence de supprimer state.yml, lien tier de
  compte SS ↔ maxthreads. Se combine avec PLAN_DOCUMENTATION.md. *(petit)*
- [ ] **P2.6 — Mode non-interactif `--plain`** + `--resume=yes|no` : aucune détection
  de tty aujourd'hui (`Ui::new` fait raw mode inconditionnellement) ; 3 bloqueurs
  CI : prompt resume, TUI, modale. Indispensable pour le cas d'usage CI du README. *(moyen/gros)*
- [ ] **P2.7 — Refonte TUI « turn 4 » (design handoff Claude Design)** : spec complète
  dans `tmp/design_handoff_rompom_tui/` (README.md + mockups.dc.html, maquettes `4a/4b/4c`).
  Supprime la découpe par phase (`PANELS`/`RomPhase`/`render_active()` disparaissent) au
  profit d'une grille unique : une ligne par ROM à sa place d'arrivée, colonnes d'état
  (id/pkg/rom + 9 pastilles médias), bandeau progression + sparkline débit + ETA,
  sélection avec lignes de détail, vues filtrées erreurs (`e`) et à-identifier (`m`,
  avec `a` = accepter les candidats ≥ 90 %), modal réaligné, vue repliée < 100 colonnes,
  bilan de fin dans l'alternate screen (quitter avec `q`, `Summary::print()` en repli
  non-interactif). Haute fidélité : couleurs/largeurs/raccourcis définitifs, garder les
  glyphes Nerd Font de `MEDIA_ICONS`. Fichiers : `ui/render.rs` (réécrit), `ui/mod.rs`,
  `ui/modal.rs`, `summary.rs`. *(gros)*
  - Prérequis : P1.2 (propager les messages d'erreur — la vue erreurs en a besoin) ;
    fournit de fait P2.3 (statut retry visible) et la moitié UI de P2.6
    (`Summary::print()` conservé comme repli non-interactif).
  - Penser à committer le handoff dans le dépôt (ex. `design/tui/`) — il est
    actuellement dans `tmp/`, non versionné.

## P3 — Dette et long terme

- [ ] Dédupliquer : calcul `rom_unchanged` (×2 dans discovery.rs), liste des 8 médias
  (×4 → un `Medias::iter()`), `copy_rom`/`download_rom` (closure fetch),
  les 8 blocs de `build_pkgbuild` (table-driven).
- [ ] Utiliser `m.url` au lieu de reconstruire les URLs SS à la main dans
  `build_pkgbuild` (cassera au premier changement de format d'URL SS).
- [ ] `enum StepError { Interrupted, Transient, Fatal }` au lieu de la sentinelle
  `Err("interrupted")`.
- [ ] Nettoyer le code mort : `StepData` quasi entier, `Phase`/`StepKind::phase()`,
  `Package.name` ≡ `Package.rom`.
- [ ] Fuite de permit `modal_sem` sur chemin d'erreur (`discovery.rs:355-384`) → guard
  RAII ; `debug_assert!` anti-wrap dans `dec_wait_for` ; `cancelled` sous le mutex
  du Semaphore (supprime le polling 50ms) ; retry sans `thread::sleep` bloquant.
- [ ] Templates : `mkdir -p 0700 -p` (voulu : `-m 0700`) et `ls *.pdf,` (virgule
  parasite) dans multidisc/psx/ps2-package.jinja ; `sed` Sega CD cassable par `|`
  dans un nom de fichier.
- [ ] Migration rustls (rompom + screenscraper + internetarchive) → supprime openssl
  vendored + perl du Nix. Remplacer `serde_yaml` (archivé). `Debug` masqué sur
  `Auth`/`ScreenScraper`.
- [ ] Migration clap ; regrouper les fichiers d'état dans `.rompom/` (avec migration) ;
  checks Nix clippy + cargo test + `--edition 2021` sur le check rustfmt.
- [ ] Features planifiées : contribution SS (PLAN_SS_ROM_CONTRIBUTION.md — après P0,
  ajoute un step au DAG) ; refonte README (PLAN_DOCUMENTATION.md, avec P2.5).

---

## Séquencement proposé

1. **v0.15.1** — P0 complet + P1.1-P1.3 : patch sécurité/fiabilité, aucun changement
   de comportement visible.
2. **v0.16** — outillage (P1.4-P1.6, CI) + UX rapide (P1.7-P1.8, P2.1-P2.3, P2.5).
3. **v0.17** — `--plain` (P2.6), multi-disc edge cases (P2.4), `--init` (P2.2).
4. **v0.18** — refonte TUI « turn 4 » (P2.7), une fois P1.2 en place et l'état stabilisé.
5. **Ensuite** — dette P3 au fil de l'eau, puis contribution SS sur base saine.
