# Tests manuels — v0.20.0 (progression des téléchargements + dette P3)

> Le pourcentage et le débit ne se vérifient que sur un vrai téléchargement, dans un vrai
> terminal. Rien de tout ça n'est jouable depuis l'environnement de développement de
> Claude : pas de terminal de contrôle, pas de compte ScreenScraper.
>
> Les deux bugs de templates et les refactors sont, eux, couverts par les snapshots.

---

## 1. Avancement d'un téléchargement

| # | Test | Attendu |
|---|---|---|
| 1.1 | Un run sur une source Internet Archive, ROMs de plusieurs dizaines de Mo | La colonne `rom` passe du spinner à `12%`, `37%`, … puis `✓`. Le chiffre monte, ne recule pas. |
| 1.2 | Une ROM déjà présente et valide | `=` gris, **aucun** pourcentage : rien ne transite. |
| 1.3 | Une source dossier (`folder`) | La colonne `rom` reste au spinner pendant la copie — `fs::copy` ne rend pas la main, c'est voulu. |
| 1.4 | Les médias | Les pastilles passent en `◐` puis vertes ; le débit bouge pendant une vidéo (c'est le plus gros média). |
| 1.5 | Couper le réseau au milieu d'une grosse ROM | Le pourcentage se fige, puis `retrying (n/3)…`, puis repart. **Pas de panique.** |
| 1.6 | Un miroir IA qui bascule sur un autre | Le pourcentage **repart de zéro** sans que le volume total ne recule ni ne s'emballe (c'est le cas que le garde-fou anti-recul traite). |

## 2. Débit, volume, ETA

| # | Test | Attendu |
|---|---|---|
| 2.1 | Regarder `MiB/s` pendant une grosse ROM | Il bouge **en continu**, plus par paliers comme en v0.19. |
| 2.2 | Volume total en fin de run | Cohérent avec la somme des fichiers réellement téléchargés — et **pas le double** (le piège : le compter au fil de l'eau *et* à la fin). |
| 2.3 | Un run entièrement `unchanged` | Volume ≈ 0, débit ≈ 0, pas de division par zéro à l'écran. |
| 2.4 | ETA | Toujours calculé sur la dernière minute, toujours `—` quand rien ne finit. |

## 3. Ligne de détail

| # | Test | Attendu |
|---|---|---|
| 3.1 | Sélectionner un ROM en cours de téléchargement | Ligne 3 = `rom 2.4/3.9 MiB`, qui avance. |
| 3.2 | Sélectionner un ROM terminé | Ligne 3 = le répertoire de sortie, comme en v0.19. |
| 3.3 | Sélectionner un ROM en échec | Ligne 3 = la cause entière. |

## 4. Les deux bugs de templates

| # | Test | Attendu |
|---|---|---|
| 4.1 | `makepkg` sur un paquet PSX / PS2 / multi-disc | **Aucun répertoire nommé `0700`** dans le paquet, et le répertoire de données a bien le mode 0700. |
| 4.2 | Un jeu avec un manuel | `manual.pdf` est **installé** — avant, `ls` cherchait un fichier nommé `*.pdf,` et le manuel était silencieusement perdu. |
| 4.3 | `ls` dans le log de build | Plus de `ls: cannot access '*.pdf,'`. |

## 5. Le lien média — le plus important de cette liste

| # | Test | Attendu |
|---|---|---|
| 5.0 | Un run complet sur un système entier | **Aucun blocage ScreenScraper**, et les neuf pastilles renseignées sur chaque ROM. Avant, chaque média passait par `mediaJeu.php` et quatre types sur huit tombaient en 404. |
| 5.0b | Une URL du PKGBUILD vs ce que rompom télécharge | Le **même** lien : `media_url()` est la seule définition des deux côtés. |
| 5.0c | La région dans les URLs | Entre parenthèses : `sstitle(jp).png`, `manuel(us).pdf`. Jamais collée. |
| 5.0d | Un média qui échoue puis réussit au retry | Les autres médias sont **quand même** téléchargés — avant, la reprise trouvait `medias` vide et sautait toute la boucle en annonçant `done`. |
| 5.0e | `Mike Tyson's Punch-Out!!` sur NES | Le manuel est récupéré (il existe sur la fiche SS). |

## 5bis. `m.url`

| # | Test | Attendu |
|---|---|---|
| 5.1 | Un PKGBUILD généré | Les 8 sources média pointent sur les URLs renvoyées par ScreenScraper. **Aucun `devid`, `devpassword`, `ssid`, `sspassword` nulle part dans le fichier.** |
| 5.2 | `grep -ri 'password' <system>/*/PKGBUILD` | Rien. |
| 5.3 | `makepkg` sur un paquet complet | Les médias se téléchargent — les URLs sont valides. |
| 5.4 | Si un step échoue sur une URL refusée | Le message dit qu'une URL portait des credentials **sans citer l'URL**. |

## 6. Non-régression

| # | Test | Attendu |
|---|---|---|
| 6.1 | `--plain` | Sortie identique à v0.19. |
| 6.2 | Reprise après Ctrl-C | Inchangée. |
| 6.3 | Un run entièrement `unchanged` relancé deux fois | Aucun `pkgver` bumpé — les refactors de `Package` et `StepData` ne doivent rien changer au calcul de delta. |
| 6.4 | `description.xml` d'un jeu connu | Octet pour octet identique à v0.19 (snapshot). |
| 6.5 | Vue erreurs | `Transfer interrupted on …` est rangé dans `download`, pas dans `other`. |
