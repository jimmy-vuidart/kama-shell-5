# 13 — Une fenêtre démappée sans être détruite reste en fantôme

- **Priorité :** faible
- **Nature :** bug (protocole xdg-shell)
- **Zone :** `crates/compositor/src/handlers/compositor.rs`, `shell.rs`
- **Se vérifie :** en imbriqué, avec un client d'essai à écrire
- **Certitude :** lu dans le code et dans Smithay ; rare en pratique

## Constat

Le protocole xdg-shell permet à un client de cacher sa fenêtre en attachant
un tampon nul, sans détruire son `xdg_toplevel`. Smithay remet alors le rôle
à zéro (`wayland/shell/xdg/mod.rs:1615`, « got unmapped ») : le client doit
refaire un commit sans tampon et attendre un nouveau configure initial.

Le compositeur ne retire une fenêtre que sur `toplevel_destroyed`
(`handlers/xdg_shell.rs:60`). Et `map_window` (`shell.rs:460`) ne cherche que
parmi les toplevels jamais placés.

## Conséquence

- La fenêtre reste dans le modèle sans rien afficher : son cadre, sa barre de
  titre et sa capsule restent à l'écran.
- Quand le client veut la remontrer, personne ne lui envoie son configure
  initial. Il attend, jusqu'à ce qu'un `apply_arrangement` sans rapport le lui
  envoie par hasard.

La plupart des toolkits (GTK, Qt, SDL récent, Chromium) détruisent le
toplevel pour cacher une fenêtre : le cas est rare.

## Piste

Dans `commit`, pour une fenêtre gérée dont la surface racine n'a plus de
tampon : la retirer du modèle (`remove_window`) et remettre sa `Window` dans
`shell.unmapped`. Son prochain commit repasse alors par `map_window`, qui la
place et la configure.

## Terminé quand

- Un petit client d'essai (quelques lignes, à ranger avec les autres outils
  d'essai) ouvre une fenêtre, attache un tampon nul, puis la remontre.
- Essai scripté en imbriqué : après le tampon nul, la capture ne montre plus
  ni cadre ni capsule ; après la remise, la fenêtre est de retour.

## Liens

- Voisin de : 14 (même chemin d'ouverture).
