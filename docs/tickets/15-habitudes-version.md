# 15 — Les habitudes n'ont ni version ni copie de secours

- **Priorité :** faible
- **Nature :** robustesse
- **Zone :** `crates/compositor/src/persist.rs`, `crates/model/src/memory.rs`
- **Se vérifie :** par tests unitaires
- **Certitude :** lu dans le code

## Constat

`memory.json` est la table des habitudes écrite telle quelle
(`#[serde(transparent)]`, `memory.rs:20`), sans numéro de version.

`load` (`persist.rs:23`) traite un fichier illisible comme l'absence
d'habitudes, avec un avertissement. La sauvegarde suivante l'écrase.

`save` (`persist.rs:36`) écrit à côté puis déplace, sans `fsync` avant le
déplacement.

## Conséquence

- Le jour où le format change (instantanés, ratio mémorisé par écran…), une
  version plus ancienne ou plus récente du compositeur lit un fichier qu'elle
  ne comprend pas, repart de zéro, et écrase tout à la première fenêtre
  rangée.
- Un fichier abîmé est perdu sans laisser de quoi le réparer.
- Après une coupure de courant, le fichier peut se retrouver vide.

## Piste

1. Écrire `{ "version": 1, "habits": { … } }`, et lire aussi l'ancien format
   (la table nue).
2. Devant un fichier illisible ou d'une version inconnue : le renommer
   (`memory.json.broken`) avant de continuer, et ne pas l'écraser.
3. `sync_all` sur le fichier temporaire avant `rename`.

## Terminé quand

Tests unitaires dans `persist.rs` :

- un fichier à l'ancien format est lu, puis réécrit au nouveau ;
- un fichier illisible est mis de côté, et retrouvé intact après une
  sauvegarde ;
- un fichier d'une version plus récente n'est ni lu ni écrasé.

## Liens

- À faire avant de donner une forme aux instantanés (règle 6 de `CLAUDE.md`).
