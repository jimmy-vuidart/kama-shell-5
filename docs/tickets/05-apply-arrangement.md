# 05 — `apply_arrangement` fait tout et redessine tout

- **Priorité :** moyenne
- **Nature :** dette, performance
- **Zone :** `crates/compositor/src/shell.rs`, `input.rs`, `render/clipped.rs`
- **Se vérifie :** en imbriqué
- **Certitude :** lu dans le code ; la raison d'être de `full_redraw` est une hypothèse

## Constat

`State::apply_arrangement` (`shell.rs:666`) est le seul chemin entre le
modèle et Wayland. À chaque appel, quel que soit le changement, il :

- reprend toute la géométrie du modèle et retrie les fenêtres ;
- dit à chaque fenêtre, pour chaque écran, si elle y est ;
- prépare un configure pour chaque fenêtre ;
- recalcule le focus du clavier ;
- compare les habitudes à celles du disque, et les écrit si elles diffèrent ;
- marque l'état du shell comme périmé ;
- lève `full_redraw` (`shell.rs:725`), qui remet à zéro les tampons des
  **trois** écrans à l'image suivante (`udev.rs:537`).

Il est appelé à chaque mouvement de souris pendant un déplacement
(`input.rs:742`), un redimensionnement (`input.rs:685`) ou un réglage de
ratio (`input.rs:727`), et à chaque changement de focus.

C'est aussi le symptôme d'un `State` trop large : une quarantaine de champs,
des blocs `impl State` dans douze fichiers.

## Conséquence

- Glisser une fenêtre sur un écran latéral fait repeindre en entier les trois
  écrans à chaque mouvement, celui du jeu compris.
- Le suivi des dégâts ne sert plus dès qu'on touche à une fenêtre.
- Toute nouvelle règle passe par cette fonction, qui grossit.

## Piste

1. **Dire ce qui a changé.** Comparer l'ancienne et la nouvelle
   `WindowGeometry` de chaque fenêtre : ne reconfigurer que celles qui ont
   bougé, ne marquer que les écrans touchés (drapeaux par écran du ticket 01).
2. **Se passer de `full_redraw`.** Il compense sans doute un suivi des dégâts
   incomplet : `ClippedSurface` (`render/clipped.rs`) reprend les dégâts de sa
   surface, donc un changement de rognage ou de rayon seul ne déclare rien.
   Le faire déclarer par l'élément lui-même, puis retirer le drapeau de
   `apply_arrangement`. Il reste utile pour le thème et le retour de VT.
3. **Sortir la sauvegarde des habitudes** du chemin commun : seulement après
   les actions qui changent la mémoire (ranger, faire flotter, oublier).

## Terminé quand

- Pendant un glisser d'une fenêtre flottante, les écrans où rien ne bouge ne
  sont pas redessinés (compteur dans le journal, essai scripté).
- Les captures des essais scriptés existants sont identiques avant et après.
- Un changement de focus n'envoie de configure qu'aux deux fenêtres
  concernées.

## Liens

- Dépend de : 01 (drapeaux par écran).
