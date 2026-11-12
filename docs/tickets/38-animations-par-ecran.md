# 38 — Les animations redessinent tous les écrans

- **Priorité :** faible
- **Nature :** performance
- **Zone :** `crates/compositor/src/animate.rs` (`keep_animating`)
- **Se vérifie :** en imbriqué (compteur), sur le matériel (jeu)
- **Certitude :** lu dans le code

## Constat

Tant qu'une animation court, `keep_animating` demande une image à **tous**
les écrans (`queue_redraw`). Les écrans où rien ne bouge dessinent des images
vides : peu coûteuses depuis le ticket 01 (leur attente est calée sur le
vblank), mais pas gratuites (construction de la scène, flou consulté). Pendant
un jeu au centre, ouvrir une fenêtre à gauche fait aussi travailler l'écran du
jeu.

## Piste

Que chaque animation dise son écran : le mouvement d'une fenêtre (écran de
départ et d'arrivée), un fantôme (son écran), un panneau (le sien), les zones
de dépôt (tous : elles s'y affichent), le sélecteur Alt+Tab (celui du
pointeur). `keep_animating` appelle alors `queue_redraw_on` pour ceux-là.

## Terminé quand

- Test : une fenêtre qui s'ouvre à gauche ne demande d'image qu'à l'écran
  gauche.
- Les captures ralenties (`--animation-slowdown`) sont inchangées.

## Liens

- Suit : 01.
