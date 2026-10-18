# 26 — Pas de raccourci pour fermer la fenêtre active

- **Priorité :** faible
- **Nature :** évolution
- **Zone :** `crates/compositor/src/shortcuts.rs`
- **Se vérifie :** par tests et en imbriqué
- **Certitude :** lu dans le code

## Constat

Les raccourcis (`shortcuts.rs`) ne savent pas fermer une fenêtre. Depuis les
capsules, toute fenêtre rangée ou maximisée a un bouton « fermer » ; une
fenêtre flottante a celui de sa barre, ou de la barre qu'elle dessine. Reste
une fenêtre flottante **sans aucune barre** (une appli X11 qui refuse la
décoration sans en dessiner) : seul le menu du dock (« Fermer la fenêtre »)
la ferme.

Reporté de la phase 4.

## Piste

Un raccourci `Mod+…` qui demande la fermeture de la fenêtre qui a le focus
(`send_close`, ou `WM_DELETE_WINDOW` pour X11), jamais pendant qu'un jeu
ancré a le focus. Le choix de la touche revient à Jimmy (`Mod+Q` est l'usage
le plus répandu).

## Terminé quand

- Test du raccourci dans `shortcuts.rs`.
- En imbriqué : une fenêtre Wayland et une fenêtre X11 se ferment au
  clavier ; un jeu ancré ne se ferme pas.

## Liens

- Aucun.
