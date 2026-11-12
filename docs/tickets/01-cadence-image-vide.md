# 01 — Une image vide bloquait l'écran pendant une période

- **Priorité :** haute
- **Nature :** performance (saccades en jeu, ressenti général)
- **Zone :** `crates/compositor/src/backend/udev.rs`, `state.rs`, `shell.rs`, `handlers/compositor.rs`, `input.rs`
- **Se vérifie :** sur le matériel seulement
- **État :** **fait dans le code, jamais exécuté sur le matériel**

## Ce qui était lu dans le code

1. **Le besoin de redessiner était global.** Chaque commit de n'importe
   quelle surface, chaque mouvement du pointeur et chaque `apply_arrangement`
   faisaient dessiner les trois écrans ; `apply_arrangement` les faisait en
   plus **repeindre en entier** (tampons remis à zéro, flou compris), à chaque
   mouvement de souris pendant un glisser.
2. **Une image sans changement mettait l'écran en attente** d'une période
   entière, sur un minuteur sans lien avec le vblank : un vrai changement
   arrivé entre-temps (l'image suivante d'un jeu) attendait jusqu'à une
   période de plus.

## Ce qui a été fait

- **Demandes par écran** : `State::queue_redraw_on`, `queue_redraw_for` (un
  commit ne marque que l'écran où sa fenêtre, son menu ou son panneau se
  montre ; tous quand on ne sait pas : menu X11, image du pointeur, icône de
  glisser), `queue_redraw_near` (le pointeur : l'écran quitté, l'écran
  atteint, et le voisin où son image déborde). `queue_redraw()` reste pour ce
  qui touche tout (thème, animations).
- **Repeint complet limité** : `apply_arrangement` compare la géométrie de
  chaque fenêtre avant et après (`touched_screens`, testé) et ne repeint en
  entier que les écrans où une fenêtre a bougé, changé d'état, est apparue,
  disparue ou changé de rang.
- **Image vide calée sur le vblank** : son minuteur court jusqu'au prochain
  rafraîchissement estimé à partir du dernier vblank vu
  (`animation::presentation_time`, testé), et non plus une période entière.
  Sans vblank vu, ou avec le VRR actif, une période entière comme avant.
- **Animations régulières** : elles sont échantillonnées à l'instant où
  l'image atteindra l'écran, plus à celui où elle est dessinée.

## À vérifier avec Jimmy

- Une vidéo à gauche, un jeu au centre : pas de saccade dans le jeu ; avec
  `RUST_LOG=debug`, l'écran du jeu n'est plus dessiné à chaque image de la
  vidéo.
- Glisser une fenêtre flottante sur un écran latéral pendant un jeu : le jeu
  ne saccade pas.
- Une animation (ouvrir, réduire, ranger) est fluide sur le 3440×1440.
- Rien ne reste à l'écran après un changement (fenêtre fermée, onglet changé,
  colonne élargie) : un reste voudrait dire qu'un écran attendait encore le
  repeint complet d'avant. Le dire avec l'écran et le geste.

## Reste

- La machine `Pacing` n'est toujours pas en logique pure (ticket 20).
- Les animations redessinent encore tous les écrans (ticket 38).

## Liens

- Sert à : 05, 24.
