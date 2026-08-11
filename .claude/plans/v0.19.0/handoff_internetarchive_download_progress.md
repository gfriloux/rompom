# Handoff — `internetarchive` : rapporter la progression d'un téléchargement

**Dépôt** : `github.com/gfriloux/internetarchive` (local : `../internetarchive`)
**Demandé par** : rompom v0.19.0 (P2.7, refonte TUI « turn 4 »)
**Taille** : petit (~20 lignes)
**Écrit le** : 2026-08-11

---

## Ce qui manque

`Download::fetch()` ne dit rien pendant qu'il télécharge. Tout se joue dans
`src/download.rs:88` :

```rust
fn download_url(client: &reqwest::blocking::Client, url: &str, dest: &Path) -> Result<()> {
  let mut res = client.get(url).send().and_then(|r| r.error_for_status())
    .context(DownloadFailedSnafu { url })?;
  let mut file = File::create(dest).context(IoSnafu { path: dest })?;
  res.copy_to(&mut file).context(DownloadFailedSnafu { url })?;   // ← opaque
  Ok(())
}
```

`copy_to` boucle jusqu'à la fin sans rendre la main. Entre le début et la fin d'une ROM
de 700 Mo, l'appelant n'a **aucun** moyen de savoir où en est le transfert.

## Ce que rompom ne peut pas afficher à cause de ça

La spec `design/tui/handoff.md` demande trois choses qui en dépendent, et les trois sont
sorties du périmètre de v0.19 :

1. **La cellule `62%`** dans la colonne `rom` de la grille. En v0.19 elle montre un
   spinner : « ça travaille », sans dire combien il reste.
2. **Le débit instantané** (`12.1 MiB/s`). rompom en affiche un, mais calculé à la
   granularité du **fichier terminé** : il avance par paliers, et une seule grosse ROM en
   cours ne le fait pas bouger du tout.
3. **La ligne de détail** `rom 2.4/3.9 MiB` sur le ROM sélectionné.

## Correctif proposé

Une variante de `fetch` qui prend un callback, l'actuelle devenant un appel avec un
callback vide — aucune rupture pour les consommateurs existants :

```rust
impl<'a> Download<'a> {
  /// Taille annoncée par les métadonnées de l'item, si elle y est.
  pub fn size(&self) -> Option<u64> { … }

  pub fn fetch(&self, dest: &Path, method: DownloadMethod) -> Result<()> {
    self.fetch_with_progress(dest, method, |_, _| {})
  }

  /// `progress(lus, total)` est appelé au fil de la lecture. `total` vaut `None` quand
  /// le serveur n'annonce pas de `Content-Length`.
  pub fn fetch_with_progress(
    &self,
    dest: &Path,
    method: DownloadMethod,
    progress: impl FnMut(u64, Option<u64>),
  ) -> Result<()> { … }
}
```

Implémentation : remplacer `res.copy_to(&mut file)` par une boucle
`res.read(&mut buf)` → `file.write_all(&buf[..n])` → `progress(total_lu, len)`, avec un
tampon de 64 Kio. `res.content_length()` donne le total.

**Contrainte importante** : le callback est appelé depuis le thread du worker, plusieurs
fois par seconde. Il doit rester `FnMut` (pas `Fn`) et ne rien exiger de plus que `Send`
côté appelant. Ne pas le rendre `async` ni introduire de canal : rompom écrit dans un
`Mutex<AppState>` qu'il tient déjà.

Ne **pas** appeler le callback à chaque octet : un appel tous les 64 Kio suffit largement
pour une barre qui se redessine à 12 images par seconde.

## Ce que rompom activera au retour

- Une variante `Cell::Progress(u8)` dans `src/ui/mod.rs` (aujourd'hui volontairement
  absente : une variante que rien ne construit est du code mort). La colonne `rom` fait
  déjà 6 cellules de large, calibrée pour `62%` — rien à redécouper.
- `RomBar::rom_progress(read, total)`, appelée depuis le callback dans
  `handlers/downloads.rs`.
- Le débit de `ui/rate.rs` passera à la granularité de l'octet : la structure `Rate` prend
  déjà un cumul d'octets par tick, il n'y a que la source à changer.

## Après la sortie

Nouveau tag → mettre à jour dans rompom :
1. `tag = "vX.Y.Z"` dans `Cargo.toml`
2. `cargoLock.outputHashes` dans `packages/rompom/default.nix` (lancer `nix build`,
   recopier le hash `got:`)

Voir `CLAUDE.md` § *Internal libs*.
