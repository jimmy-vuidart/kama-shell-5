# 05 — `apply_arrangement` fait tout

- **Priorité :** moyenne
- **Nature :** dette, performance
- **Zone :** `crates/compositor/src/shell.rs`, `input.rs`, `render/clipped.rs`
- **Se vérifie :** en imbriqué
- **Certitude :** lu dans le code

## Constat

`State::apply_arrangement` est le seul chemin entre le modèle et Wayland. À
chaque appel, quel que soit le changement, il :

- reprend toute la géométrie du modèle et retrie les fenêtres ;
- dit à chaque fenêtre, pour chaque écran, si elle y est ;
- prépare un configure pour chaque fenêtre ;
- recalcule le focus du clavier ;
- compare les habitudes à celles du disque, et les écrit si elles diffèrent ;
- marque l'état du shell comme périmé.

Il est appelé à chaque mouvement de souris pendant un déplacement, un
redimensionnement ou un réglage de ratio, et à chaque changement de focus.

Le repeint complet des **trois** écrans qu'il demandait (`full_redraw`) est
désormais limité aux écrans où une fenêtre a changé (`touched_screens`,
ticket 01) : glisser sur un écran latéral ne repeint plus les deux autres.
L'écran touché, lui, est encore repeint en entier à chaque mouvement.

## Piste

1. **Se passer du repeint complet.** Il compense sans doute un suivi des
   dégâts incomplet : `ClippedSurface` (`render/clipped.rs`) reprend les
   dégâts de sa surface, donc un changement de rognage ou de rayon seul (une
   colonne qui rétrécit avant que l'appli redessine) ne déclare rien. Le
   faire déclarer par l'élément lui-même (un identifiant qui change avec le
   rognage, ou un dégât de toute sa géométrie), puis retirer le repeint
   complet d'`apply_arrangement`. À essayer avec des captures : un reste à
   l'écran est pire qu'un repeint de trop.
2. **Ne reconfigurer que ce qui a changé** : `touched_screens` sait déjà
   quelles fenêtres ont bougé.
3. **Sortir la sauvegarde des habitudes** du chemin commun : seulement après
   les actions qui changent la mémoire (ranger, faire flotter, oublier).

## Terminé quand

- Pendant un glisser d'une fenêtre flottante, seule la zone qu'elle quitte et
  celle qu'elle couvre sont redessinées (compteur dans le journal, essai
  scripté).
- Les captures des essais scriptés existants sont identiques avant et après.
- Un changement de focus n'envoie de configure qu'aux deux fenêtres
  concernées.

## Liens

- Suit : 01.
