# Capsules : les onglets sortent des barres de titre

Date : 2026-10-12. Statut : réalisé (voir « Écarts à la réalisation » en fin de document).

## Pourquoi

Les onglets vivent aujourd'hui dans la barre de titre de la fenêtre affichée.
Trois défauts, relevés à l'usage :

- **la double barre** : une appli qui dessine sa propre barre (Nautilus,
  Steam) récupère celle du shell dès qu'elle partage son emplacement, juste
  pour montrer les onglets ;
- **pas de vue d'ensemble** : on ne voit ce qu'il y a sur un écran que pile
  par pile, et seulement quand une pile contient deux fenêtres ;
- **l'apparence** : des onglets dans une barre font « navigateur ».

## Ce qui change, en une phrase

Chaque emplacement occupé porte, au-dessus de lui, un **groupe de capsules**
(une par fenêtre de sa pile) ; la barre de titre du shell ne sert plus qu'aux
applis qui n'ont aucune barre à elles.

Deux suppressions accompagnent ce changement : l'échange avec le centre (⇄)
et la disposition `Tabs`.

## Règles

### Où sont les capsules

- Un groupe par **emplacement occupé**, centré au-dessus de lui, même quand
  il ne contient qu'une fenêtre. Un emplacement vide n'a pas de groupe.
- En haut de chaque emplacement : **32 px** de bande, **8 px** d'espacement,
  puis la fenêtre. Le chrome passe de 46 à 48 px pour une appli qui dessine
  sa barre, et à 86 px pour une appli qui reçoit celle du shell.
- La bande appartient à l'emplacement : en `Columns` et `TwoThirdsOneThird`
  les deux groupes sont en haut de l'écran ; en `Rows`, le groupe du bas est
  au milieu de l'écran, juste au-dessus de son emplacement.
- Le groupe suit son emplacement quand le ratio change.
- Les zones de dépôt d'un glisser restent les emplacements entiers.
- **Pas de capsule** pour une fenêtre flottante, sur un écran `Free`, ni sur
  un écran ancré par un jeu.

### Ce que montre une capsule

- L'icône et le nom de l'appli (les mêmes que la barre de titre aujourd'hui).
- La capsule de la **fenêtre affichée** est pleine, son nom en demi-gras, et
  porte deux boutons : réduire, fermer.
- Les autres capsules sont atténuées ; leurs deux boutons apparaissent au
  survol, à une place qui leur est réservée dans la capsule.
- Une fenêtre **réduite** garde sa capsule, grisée. Si toutes les fenêtres
  d'un emplacement sont réduites, le groupe reste là (aujourd'hui, la seule
  fenêtre d'un emplacement, une fois réduite, ne laisse rien à l'écran).
- La capsule affichée de la **fenêtre active** est entourée de l'anneau
  d'accent.
- Largeur d'une capsule : celle de son contenu, le nom ne dépassant pas
  160 px. Quand le groupe dépasse la largeur de l'emplacement, les capsules
  les plus larges rétrécissent d'abord (le nom est coupé), jusqu'à ne montrer
  que l'icône.

### Ce qu'on y fait à la souris

| Geste | Effet |
|---|---|
| Clic sur une capsule | Affiche la fenêtre et lui donne le focus ; ramène une fenêtre réduite |
| Glisser une capsule | Déplace sa fenêtre : emplacements des trois écrans affichés, relâcher range |
| Bouton réduire | Réduit la fenêtre |
| Bouton fermer | Ferme la fenêtre, affichée ou non |

Les capsules sont du chrome du shell : la règle d'or de la souris n'est pas
touchée. `Mod+Page suiv./préc.` change toujours d'onglet.

### La barre de titre du shell

- **Fenêtre rangée** : seulement si l'appli n'a pas de barre à elle (kitty).
  Elle montre l'icône, le nom et le sous-titre, **sans bouton**, et se glisse
  comme aujourd'hui. Une appli qui dessine sa barre n'en reçoit jamais, même
  dans un emplacement partagé : l'exception de la règle « pas de double
  barre » disparaît.
- **Fenêtre flottante** : rien ne change, sauf le bouton ⇄ qui disparaît.
  La barre du shell (pour les applis sans barre) garde réduire et fermer.

### Suppressions

- **⇄, complètement** : le bouton, `Mod+C`, la règle 5, et le code d'échange
  du modèle.
- **« Au centre »**, dans la barre de rangement, reposait sur l'échange. Il
  reste, avec un sens plus simple : la fenêtre est **rangée au centre**, dans
  l'emplacement utilisé en dernier, en onglet s'il est occupé (règle 3). La
  fenêtre du centre ne bouge pas. Si le centre est en écran libre, la fenêtre
  y flotte. Toujours absent tant qu'un jeu tient le centre.
- **Disposition `Tabs`** : identique à `Full` dès que les capsules sont
  toujours visibles. Elle est supprimée et les raccourcis se resserrent :

  | Raccourci | Avant | Après |
  |---|---|---|
  | `Mod+1` | Plein | Plein |
  | `Mod+2` | Côte à côte | Côte à côte |
  | `Mod+3` | Haut / bas | Haut / bas |
  | `Mod+4` | Onglets | Écran libre |
  | `Mod+5` | Écran libre | Deux tiers + un tiers |
  | `Mod+6` | Deux tiers + un tiers | (rien) |

## Apparence

- Groupe : rectangle arrondi de 32 px de haut (rayon 16), fond `capsule_box`
  du thème (celui du dock, à 88 % en sombre et 90 % en clair), cerné d'un
  filet d'or plus clair sur l'arête haute, 3 px de marge intérieure.
- Capsule : 26 px de haut (rayon 13), icône de 14 px, nom en 13 px comme les
  barres de titre, 4 px entre deux capsules.
- Capsule affichée : fond `button_hover`, texte `text` ; les autres sans
  fond, texte `text_muted` ; réduite : texte `disabled`, icône à 45 %.
- Anneau d'accent de 2 px autour de la capsule affichée de la fenêtre active :
  une « pièce fine » d'or, dessinée dans la marge du groupe, dont il reste
  1 px (voir `docs/gold/`).
- Boutons : 20 px, survol `button_hover`, et `close_hover` pour fermer.
- Thèmes clair et sombre, police Figtree, comme le reste des décorations.

Les maquettes de `docs/maquettes/shell/` restent la référence pour les
fenêtres, mais plus pour les onglets ni pour les boutons de la barre.

## Réalisation

Les capsules sont dessinées **par le compositeur**, avec les outils des
barres de titre (`tiny-skia` et `cosmic-text` vers une texture). Pas de
programme séparé : un glisser commencé sur une capsule doit devenir un
glisser du compositeur, et le groupe doit suivre le ratio image par image.

### `crates/model`

- `Metrics` : `capsule_height` (32).
- `LayoutKind::Tabs` retiré.
- `Desk::geometry` : pour une fenêtre rangée, le cadre est l'emplacement
  moins la bande et son espacement ; `titlebar` vaut `None` dès que la
  fenêtre a sa propre barre, sans regarder la pile.
- `Desk::tab_strip` remplacé par `Desk::capsule_groups()` : pour chaque
  emplacement occupé d'un écran non ancré, son `SlotId`, le rectangle de sa
  bande, ses fenêtres dans l'ordre et celle qui est affichée.
- Retirés : `swap_with_center`, `can_swap_with_center`, `center_swap_slot`,
  et les erreurs qui ne servent qu'à eux.
- Ajouté : `Desk::send_to_center(id)` pour « Au centre » (emplacement du
  centre utilisé en dernier, sinon le premier ; flottant si le centre est
  `Free` ; refusé si le centre est ancré).

### `crates/compositor`

- `decoration/capsules.rs` (nouveau) : géométrie pure d'un groupe. Entrées :
  la bande, et pour chaque capsule la largeur voulue de son nom et si ses
  boutons sont montrés. Sorties : le rectangle du groupe, de chaque capsule
  et de chaque bouton ; `capsule_at` et `button_at` pour les clics. Partagé
  par le dessin et les clics, testé comme `decoration/layout.rs`.
- `decoration/paint.rs` : `Painter::capsule_group`, une texture par groupe,
  repeinte quand sa description change (noms, fenêtre affichée, survol,
  focus, réduites, thème). Le dessin des onglets et du bouton ⇄ est retiré.
- `decoration/layout.rs` : plus d'onglets ni de ⇄ ; les boutons de la barre
  ne sont posés que pour une fenêtre flottante.
- `shell.rs` : les groupes sont tenus à côté des fenêtres (`show_swap` et
  `tabs` quittent `ManagedWindow`) et rafraîchis avec l'arrangement.
- `render/scene.rs` : les groupes sont dessinés avec les fenêtres rangées,
  donc sous les fenêtres flottantes, la barre de rangement, les menus et les
  panneaux.
- `input.rs` : le test de clic connaît les capsules (même ordre que le
  dessin) ; clic, glisser (le geste `Placing` existant), boutons, survol.
- `shortcuts.rs` : `SwapWithCenter` retiré, `Mod+4` et `Mod+5` décalés.
- `placement.rs` : « Au centre » appelle `send_to_center`.

### `crates/ipc` et `crates/dock`

`LayoutKind` voyage dans l'état poussé au dock : la valeur `tabs` disparaît.
Les deux programmes sont compilés et installés ensemble ; aucun état
enregistré ne contient de disposition. Rien d'autre à changer dans le dock.

## Cas limites

- **Emplacement étroit** (20 % d'un écran 16:9, environ 370 px) avec
  beaucoup de fenêtres : les capsules se réduisent à leur icône ; au-delà,
  les dernières sont rognées par le bord du groupe. Pas de défilement.
- **Écran ancré** : `capsule_groups` ne rend rien pour lui.
- **Appli X11 qui change d'avis** sur sa décoration : la barre du shell
  apparaît ou disparaît comme aujourd'hui, la capsule ne bouge pas.
- **Glisser en cours** : la capsule de la fenêtre glissée reste à sa place
  jusqu'au relâchement.

## Vérification

- Tests unitaires du modèle : cadre d'une fenêtre rangée (bande retirée),
  pas de barre pour une appli à barre propre dans un emplacement partagé,
  groupes par disposition (dont `Rows`), aucun groupe sur un écran ancré ou
  `Free`, groupe conservé quand tout est réduit, `send_to_center`.
- Tests unitaires de `decoration/capsules.rs` : centrage, largeur maximale,
  rétrécissement, boutons, clics.
- Tests des raccourcis mis à jour.
- Essais scriptés en imbriqué, captures relues, thèmes clair et sombre :
  deux kitty et une appli GTK dans le même emplacement ; clic sur une
  capsule, survol, réduire, fermer, glisser une capsule vers un autre écran,
  réglage du ratio, « Au centre », plein écran (les capsules disparaissent).
- `scripts/ci.sh` propre.

## Documentation à mettre à jour

`CLAUDE.md` (dispositions, règle 5, précisions du mode jeu, apparence,
raccourcis, états des phases 2 et 3), `docs/tasks/PHASE_4.md` (décisions),
`config/config.toml` s'il cite les raccourcis.

## Écarts à la réalisation

- **Place des boutons réservée.** Le plan prévoyait que les boutons d'une
  capsule non affichée recouvrent la fin de son nom au survol. À l'essai, un
  clic visant le nom d'une capsule courte (« kitty ») tombait sur « fermer ».
  Chaque capsule réserve donc 44 px à droite pour ses boutons, visibles
  seulement sur la capsule affichée et sous le pointeur.
- **Survol** : une capsule non affichée prend un fond léger sous le pointeur.
- **« Au centre » crée une habitude**, comme « Ranger ici ».
- Une capsule de moins de 72 px n'a pas de bouton.

## Hors périmètre

- Changer de disposition à la souris.
- Réordonner les capsules d'un groupe en les glissant.
- Capsules pour les fenêtres flottantes.
- Vignettes au survol.
