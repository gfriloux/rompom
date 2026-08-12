# Phase 0 — audit du dépôt avant de coder (v0.20.0)

Exécuté le 2026-08-11, avant toute modification.

## 1. Portes de qualité

```
nix develop --command just ci
```

**Vert, code de sortie 0.** **146 tests**, 0 échec. Working tree propre, `master` @
`904c8f8` (`Merge branch 'feat/v0.19.0'`), tag `v0.19.0` posé.

## 2. Les deux libs voisines livrent ce qui était demandé

| lib | tag | ce qui arrive |
|---|---|---|
| `internetarchive` | **v0.3.0** | `Download::fetch_with_progress(dest, method, FnMut(u64, Option<u64>))`, `Download::size()` |
| `screenscraper` | **v0.8.0** | `MediaDownload::fetch_with_progress(dest, FnMut(u64, Option<u64>))` |

Les deux ont gardé l'ancien `fetch` en appelant le nouveau avec un callback vide : aucune
rupture, le bump peut se faire seul avant de brancher quoi que ce soit. Les deux appellent
le callback une fois **avant** le premier octet (donc `total` est connu tout de suite),
puis tous les 64 Kio.

## 3. Trois choses relevées dans les diffs

### 3.1 Le compteur peut reculer — c'est le vrai piège

`internetarchive` le documente noir sur blanc (`src/download.rs:78`) :

> On server fallback the destination file is truncated and `read` restarts from 0, so the
> callback may see the counter go backwards.

`fetch_https` boucle sur `metadata.file_urls()` et rappelle `download_url` sur le miroir
suivant, qui refait `File::create(dest)` — le fichier est tronqué et `read` repart à zéro.

rompom alimente `ui::rate::Rate` avec un **cumul** d'octets dont il calcule des deltas
(`bytes - self.last_bytes` dans `Rate::tick`). Une soustraction `u64` sur un compteur qui
recule panique en debug et boucle en release. C'est le point à traiter en premier dans
l'étape « débit à l'octet », et il a son test.

### 3.2 `Error::TransferFailed` est une variante neuve côté IA

`#[snafu(display("Transfer interrupted on {}: {}", url, source))]`. Rien à mettre à jour :
rompom ne fait aucun `match` exhaustif sur l'erreur IA, tout passe par
`StepError::transient`. Mais `errors::classify()` ne la range dans `download` que **par
accident** — parce que le message contient `https`, qui matche son test `contains("http")`.
Un test la pinne dans l'étape correspondante.

### 3.3 `.without_url()` n'a pas été fait côté screenscraper

`Error::Request` porte toujours un `reqwest::Error` dont le `Display` ajoute l'URL
complète, credentials compris. rompom reste protégé (il compose ses phrases depuis
`ApiFailure`), le prochain consommateur ne le sera pas. C'était un point **distinct** du
handoff, il reste en P3.

## 4. `m.url` porte les credentials — l'item P3 est faux, et il y a une fuite vivante

**Établi en lisant `package.rs:48`**, après que le mainteneur a fourni les URLs attendues :

```rust
fn media_region(url: &str) -> &str {
  url.find("media=").map(|i| &url[i + 6..]).unwrap_or("")
}
```

Pour que `image.png::…/sstitlejp.png` sorte de là, il faut que `x.url` contienne
`…&media=sstitlejp`. `m.url` est donc la forme **API** —
`https://api.screenscraper.fr/api2/mediaJeu.php?devid=…&devpassword=…&ssid=…&sspassword=…&media=sstitlejp`
— et `base_query()` met les deux mots de passe dans chaque requête.

### 4.1 L'item P3 « utiliser `m.url` » doit être annulé

La reconstruction à la main n'est pas de la duplication naïve : c'est un **blanchiment
délibéré** de l'URL API en URL publique. L'appliquer écrirait `devpassword` et
`sspassword` dans chaque PKGBUILD **publié**. L'entrée de `TODO.md` est réécrite en
avertissement.

### 4.2 Une fuite vivante sur le chemin médias

`worker/handlers/downloads.rs` :

```rust
.map_err(|e| StepError::Transient(format!("media {}: {}", kind, e)))?
```

`e` est `screenscraper::download::Error`, dont le `Display` est
`"Failed to download {url}: {source}"` avec `url = media.url`. Un échec de téléchargement
de média écrit donc les deux mots de passe dans la colonne `status` de la grille, la vue
erreurs, `<system>.errors.log`, `Summary::print()` et `<system>.debug.log`.

C'est exactement la classe de bug que **P1.1** a fermée sur le chemin d'identification —
le chemin médias n'avait jamais été audité. Manquement direct au garde-fou « les
credentials ScreenScraper ne sont jamais dans un log, un test, une fixture ou un message
d'erreur ». **Traité en premier dans ce lot.**

### 4.3 `media_region()` est mal nommée et fragile

Elle ne rend pas une région mais le **slug média** (`sstitlejp`, `box-2Djp`, `ss(jp)`), et
`find("media=")` prend *tout ce qui suit* : si ScreenScraper ajoutait un paramètre après
`media=`, il finirait dans le nom de fichier. Renommée et durcie dans le même lot.

## 4bis. Archive du raisonnement initial (invalidé par ce qui précède)

`TODO.md` P3 demande d'utiliser `m.url` au lieu de reconstruire les URLs SS à la main dans
`build_pkgbuild`. Ces URLs finissent dans des **PKGBUILD publiés** : si `m.url` portait des
paramètres d'authentification, s'en servir écrirait les credentials dans des fichiers que
l'utilisateur pousse sur son dépôt de paquets. Ce serait une régression, pas une
amélioration.

Ce que le dépôt permet d'établir : les URLs que rompom fabrique aujourd'hui
(`package.rs:351-412`) sont de la forme
`https://screenscraper.fr/medias/{systemeid}/{jeuid}/{media}.{ext}` — **sans credentials**,
et manifestement recopiées d'un vrai `m.url` à l'époque. Il est donc très probable que
`m.url` ait exactement cette forme. Mais « très probable » ne suffit pas pour un fichier
publié, et il n'y a pas de réponse SS réelle sous la main ici.

→ Le correctif est écrit pour **échouer fermé** : `m.url` est utilisée, et une URL portant
`devid`, `devpassword`, `ssid` ou `sspassword` fait échouer le step au lieu d'être écrite.
Même logique que `sanitize_sha1()`, qui refuse plutôt que d'inventer.

## 5. Hors périmètre, mais signalé

`../screenscraper/apiv2.html` (fichier local, **non suivi par git**) contient le login et
le mot de passe ScreenScraper du mainteneur en clair, ligne 2922 : c'est la page telle que
le site la rend à un membre connecté. Rien n'est publié ; c'est un fichier sur disque.
