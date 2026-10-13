# 06 — Plusieurs tests de survol par événement souris

- **Priorité :** moyenne
- **Nature :** performance
- **Zone :** `crates/compositor/src/input.rs`, `panels.rs`
- **Se vérifie :** en imbriqué
- **Certitude :** lu dans le code ; coût non mesuré

## Constat

Pour un seul mouvement de souris, `pointer_relative` (`input.rs:309`) appelle
`hit_test` (`input.rs:1071`) :

1. une fois pour savoir ce qui est sous le pointeur avant le mouvement ;
2. une fois par point essayé si le pointeur est confiné (`is_inside`) ;
3. une fois dans `pointer_moved` ;
4. une fois dans `refresh_constraint`.

Chaque `hit_test` appelle `State::panels` (`panels.rs:57`), qui alloue une
liste et prend le verrou de la carte des panneaux de chaque écran, pour
chacun des quatre calques.

Seul le cas du pointeur verrouillé (jeu en vue subjective) sort tôt, après un
seul test.

## Conséquence

Une souris de jeu envoie 1000 événements par seconde, parfois 8000. Dès que
le pointeur n'est pas verrouillé, cela fait plusieurs milliers de parcours
par seconde sur le fil unique du compositeur, celui qui dessine aussi.

## Piste

1. Calculer le `Hit` une fois par événement et le passer aux fonctions qui en
   ont besoin (`pointer_moved`, `refresh_constraint`).
2. Garder la liste des panneaux et ne la refaire que quand elle change
   (`arrange_panels`, écran ancré ou libéré).
3. Éventuellement, ne traiter que la dernière position par tour de boucle.
   **Attention :** les mouvements relatifs doivent tous partir vers le client,
   les jeux les additionnent. Seule la partie « position et survol » peut
   être regroupée.

## Terminé quand

- Un mouvement de souris sans confinement fait un seul `hit_test` (compteur
  dans un test, ou relecture).
- Les essais scriptés existants (survol, capsules, bords collants, pointeur
  verrouillé et confiné) passent sans changement.

## Liens

- Aucun.
