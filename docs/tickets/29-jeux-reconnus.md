# 29 — Seuls les jeux Steam sont reconnus comme jeux

- **Priorité :** faible
- **Nature :** évolution
- **Zone :** `crates/model` (règle « jeux au centre »), `crates/dock` (rangée Jeux)
- **Se vérifie :** par tests
- **Certitude :** décision de la phase 4, jugée insuffisante par Jimmy et
  acceptée « pour le moment » (12 octobre 2026)

## Constat

Un jeu s'ouvre au centre s'il a la classe `steam_app_*` ou s'il est lancé
depuis la rangée Jeux du tiroir. Un jeu natif, Lutris, Heroic ou un
exécutable Wine lancé autrement s'ouvre comme une appli ordinaire : flottant
sur l'écran du pointeur. La mémoire par appli corrige ensuite, une fois
rangé.

## Piste

À décider avec Jimmy. Indices possibles : la catégorie `Game` du fichier
`.desktop`, les lanceurs connus (Lutris, Heroic, `gamescope`), une fenêtre
qui demande le plein écran dès son ouverture, une liste dans `config.toml`.

## Terminé quand

- Tests du modèle pour chaque indice retenu.
- Un jeu lancé depuis Lutris ou Heroic s'ouvre au centre la première fois.

## Liens

- Aucun.
