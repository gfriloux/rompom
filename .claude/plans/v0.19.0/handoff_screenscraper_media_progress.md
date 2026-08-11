# Handoff — `screenscraper` : rapporter la progression d'un téléchargement de média

**Dépôt** : `github.com/gfriloux/screenscraper` (local : `../screenscraper`)
**Demandé par** : rompom v0.19.0 (P2.7, refonte TUI « turn 4 »)
**Taille** : petit (~20 lignes)
**Écrit le** : 2026-08-11

---

## Ce qui manque

`MediaDownload::fetch()` (`src/download.rs:40`) a exactement le même angle mort que
`internetarchive::Download::fetch` : il télécharge d'un bloc et ne dit rien tant que ce
n'est pas fini.

Le cas est moins spectaculaire que celui des ROMs — un `wheel` PNG pèse quelques dizaines
de Kio — mais **la vidéo** est un vrai fichier : `video-normalized` monte couramment à
plusieurs dizaines de Mo, et sur une bibliothèque entière c'est le poste de
téléchargement le plus lourd après les ROMs elles-mêmes.

## Ce que rompom ne peut pas afficher à cause de ça

- Le volume et le débit de rompom sont comptés **par fichier terminé**. Un run où chaque
  ROM télécharge neuf médias avance donc par paliers, et pendant une vidéo de 40 Mo le
  débit affiché ne bouge pas.
- La pastille média « en cours » (`◐`) existe déjà, mais elle ne peut pas dire *où* en
  est ce média.

## Correctif proposé

La même forme que pour `internetarchive`, pour que les deux se câblent pareil côté
rompom :

```rust
impl<'a> MediaDownload<'a> {
  pub fn fetch(&self, dest: &Path) -> Result<()> {
    self.fetch_with_progress(dest, |_, _| {})
  }

  /// `progress(lus, total)` au fil de la lecture ; `total` est `None` sans
  /// `Content-Length`.
  pub fn fetch_with_progress(
    &self,
    dest: &Path,
    progress: impl FnMut(u64, Option<u64>),
  ) -> Result<()> { … }
}
```

Boucle `read`/`write_all` avec un tampon de 64 Kio, callback tous les 64 Kio et non à
chaque octet. `FnMut`, appelé depuis le thread du worker.

**Le champ `Media::size` ne remplace pas ça** : il existe dans la réponse de l'API
(`src/jeuinfo.rs`, `pub size: Option<String>`), donne la taille attendue, et ne dit rien
de l'avancement.

## Deux choses à ne pas mélanger

Ce handoff ne concerne que la progression. La dette **`Error::Request` fuit les
credentials** (`TODO.md` P3) est un sujet distinct et indépendant : `reqwest::Error`
ajoute ` for url (<url complète>)` à son `Display`, et `base_query()` met `devpassword`
et `sspassword` dans cette URL. Correctif amont : `.without_url()` sur l'erreur avant de
la stocker (reqwest l'expose, `src/error.rs:80`). rompom est protégé — il ne cite jamais
l'erreur de la lib — mais le prochain consommateur ne le saura pas.

Si les deux partent dans la même release, tant mieux ; ils n'ont aucune raison de se
bloquer l'un l'autre.

## Après la sortie

Nouveau tag → mettre à jour dans rompom :
1. `tag = "vX.Y.Z"` dans `Cargo.toml`
2. `cargoLock.outputHashes` dans `packages/rompom/default.nix` (lancer `nix build`,
   recopier le hash `got:`)

Voir `CLAUDE.md` § *Internal libs*.
