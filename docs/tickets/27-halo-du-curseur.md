# 27 — Halo autour du curseur quand on secoue la souris

- **Priorité :** faible
- **Nature :** évolution
- **Zone :** `crates/compositor/src/input.rs`, `game.rs` (décision pure), `render/`
- **Se vérifie :** par tests et en imbriqué
- **Certitude :** absence lue dans le code

## Constat

`CLAUDE.md` (« Règle d'or de la souris ») promet qu'en secouant la souris,
un halo entoure le curseur pour le retrouver sur trois écrans. Rien ne le
fait : aucun code ne détecte la secousse.

## Piste

1. Une décision pure, testée comme `StickyEdges` : quelques changements de
   sens rapides et amples en moins d'une demi-seconde valent une secousse.
2. Un halo dessiné par le compositeur autour du pointeur, qui s'efface après
   une seconde.
3. Jamais sur un écran ancré quand le jeu a le focus, ni pointeur verrouillé.

## Terminé quand

- Tests de la détection : une secousse est reconnue, un geste ordinaire (un
  glisser, un aller-retour lent) ne l'est pas.
- En imbriqué : `--debug-input` rejoue une secousse, la capture montre le
  halo.

## Liens

- Aucun.
