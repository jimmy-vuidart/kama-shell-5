# 01 — Une image vide bloque l'écran pendant une période

- **Priorité :** haute
- **Nature :** performance (saccades en jeu)
- **Zone :** `crates/compositor/src/backend/udev.rs`, `state.rs`, `handlers/compositor.rs`
- **Se vérifie :** sur le matériel seulement
- **Certitude :** mécanisme lu dans le code ; effet ressenti à confirmer

## Constat

Deux choses se combinent.

1. **Le besoin de redessiner est global.** `State::queue_redraw`
   (`state.rs:313`) lève un seul drapeau, `redraw_needed`. Il est levé par
   chaque commit de n'importe quelle surface (`handlers/compositor.rs:108`) et
   par chaque mouvement du pointeur (`input.rs:624`). `redraw_screens`
   (`udev.rs:530`) dessine alors **tous** les écrans au repos, pas seulement
   celui où quelque chose a changé.
2. **Une image sans changement met l'écran en attente.** Dans `render_screen`,
   quand le rendu ne produit aucun dégât, rien n'est envoyé à l'écran : un
   minuteur d'une période entière est armé (`udev.rs:719` à `732`) et l'écran
   passe en `Pacing::Waiting`. Ce minuteur n'est pas calé sur le vblank. Tant
   qu'il court, un vrai changement sur cet écran attend.

## Conséquence

Cas réel du projet : une vidéo sur l'écran gauche, un jeu au centre. Chaque
image de la vidéo lève le drapeau global ; l'écran du jeu, s'il est au repos,
est rendu pour rien, puis bloqué une période (6,9 ms à 144 Hz). L'image
suivante du jeu attend la fin du minuteur.

- Avec le VRR : les images du jeu sont retardées de façon irrégulière, ce qui
  est exactement la saccade que le VRR doit supprimer.
- Sans le VRR : l'image peut manquer un vblank.

S'y ajoute un coût constant : trois constructions de scène par lot
d'événements, dont deux pour rien.

## Piste

1. **Un drapeau par écran.** Un commit ne marque que l'écran (ou les écrans)
   où sa fenêtre est visible ; un panneau, le sien ; le pointeur, l'écran où
   il était et celui où il arrive. `queue_redraw()` reste pour ce qui touche
   tout (thème, arrangement).
2. **Ne plus bloquer après une image vide.** Deux façons :
   - garder le minuteur pour les « frame callbacks » seulement, et laisser un
     écran marqué entre-temps se redessiner tout de suite ;
   - ou viser le prochain vblank estimé à partir de l'horodatage du dernier,
     comme niri (`queue_estimated_vblank_timer`, `src/backend/tty.rs`).

Le ticket 20 sort la machine `Pacing` en logique pure : à faire avant, pour
écrire ces règles avec leurs tests.

## Terminé quand

- Un commit sur un écran ne provoque aucun rendu des deux autres (test
  unitaire sur la logique de cadence).
- Après une image vide, un changement sur le même écran n'attend pas la fin
  d'une période (test unitaire).
- Sur le matériel, avec une vidéo à gauche et un jeu au centre : le journal
  ne montre plus d'images vides sur l'écran du jeu, et Jimmy ne voit pas de
  saccade.

## Liens

- Dépend de : 20.
- Sert à : 05 (drapeaux par écran), 24 (horodatage des vblanks).
