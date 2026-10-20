# Capsules

Les onglets ne sont pas dans les barres de titre : chaque emplacement occupé
porte, au-dessus de lui, un **groupe de capsules**, une par fenêtre de sa
pile. La barre de titre du shell ne sert plus qu'aux applis qui n'ont aucune
barre à elles.

Conçues le 12 octobre 2026 pour trois défauts des onglets dans la barre : la
double barre (une appli qui dessine la sienne recevait celle du shell dès
qu'elle partageait son emplacement), l'absence de vue d'ensemble d'un écran,
et l'allure de navigateur.

## Où sont les capsules

- Un groupe par **emplacement occupé**, centré au-dessus de lui, même quand
  il ne contient qu'une fenêtre. Un emplacement vide n'a pas de groupe.
- En haut de chaque emplacement : **32 px** de bande, **8 px** d'espacement,
  puis la fenêtre (`Metrics::capsule_height`).
- Le groupe suit son emplacement quand les largeurs des colonnes changent.
- Les **fenêtres maximisées** d'un écran ont leur groupe en haut au centre
  de l'écran, dans la moitié centrale de la bande. Un groupe d'emplacement
  qui le toucherait s'écarte de 8 px du côté de son milieu (à gauche s'ils
  partagent le même).
- La **seule colonne d'un écran** (quelle que soit sa disposition, onglets
  compris) compte comme maximisée, tant qu'aucune fenêtre n'y est maximisée :
  son groupe est à la place de celui des maximisées, avec « restaurer ».
- Les zones de dépôt d'un glisser restent les emplacements entiers.
- **Pas de capsule** pour une fenêtre flottante, ni sur un écran ancré par
  un jeu.

## Ce que montre une capsule

- L'icône et le nom de l'appli (ceux de son fichier `.desktop`), pas le
  titre de la fenêtre.
- La capsule de la **fenêtre affichée** est pleine, son nom en demi-gras,
  et montre ses trois boutons : réduire, agrandir (« restaurer », deux
  carrés, dans le groupe des fenêtres maximisées), fermer.
- Les autres capsules sont atténuées ; sous le pointeur, elles prennent un
  fond léger et montrent leurs boutons. La place des boutons est réservée
  dans chaque capsule : survoler ne déplace rien, et un clic sur le nom
  d'une capsule courte ne tombe pas sur « fermer ».
- Une fenêtre **réduite** garde sa capsule, grisée. Si toutes les fenêtres
  d'un emplacement sont réduites, le groupe reste là.
- La capsule affichée de la **fenêtre active** porte l'anneau d'accent.
- Largeur : celle du contenu, le nom ne dépassant pas 160 px. Quand le
  groupe dépasse la largeur de l'emplacement, les capsules les plus larges
  rétrécissent d'abord (le nom est coupé), jusqu'à l'icône seule. Une
  capsule trop étroite pour ses boutons n'en a pas.

## Ce qu'on y fait à la souris

| Geste | Effet |
|---|---|
| Clic sur une capsule | Affiche la fenêtre et lui donne le focus ; ramène une fenêtre réduite |
| Clic sur une capsule d'emplacement, quand une fenêtre est maximisée | Ramène l'écran partagé (la maximisée reste dans son groupe, atténuée) |
| Glisser une capsule | Déplace sa fenêtre : emplacements des trois écrans affichés, relâcher range |
| Réduire | Réduit la fenêtre |
| Agrandir / restaurer | Maximise la fenêtre, ou la remet à sa place ; restaurer la seule colonne d'un écran fait flotter sa fenêtre |
| Fermer | Ferme la fenêtre, affichée ou non |

Les capsules sont du chrome du shell : la règle d'or de la souris n'est pas
touchée. `Mod+Page suiv./préc.` change d'onglet.

## La barre de titre du shell

- **Fenêtre rangée ou maximisée** : seulement si l'appli n'a pas de barre à
  elle (kitty). Icône, nom et sous-titre, **sans bouton** (ils sont sur la
  capsule) ; elle se glisse comme une barre de fenêtre flottante. Une appli
  qui dessine sa barre n'en reçoit jamais, même dans un emplacement partagé.
- **Fenêtre flottante** : réduire, agrandir et fermer restent sur sa barre.

## Apparence

Mesures dans `crates/compositor/src/decoration/capsules.rs`, dessin dans
`decoration/paint.rs`.

- Groupe : pilule de 32 px de haut en verre (`glass`) avec liseré (`rim`),
  3 px de marge intérieure, 2 px entre deux capsules.
- Capsule : 26 px de haut, en pilule ; icône de 14 px, nom en 13 px (Figtree).
- Capsule affichée : fond `glass_strong`, texte `text` ; les autres sans
  fond (`hover` sous le pointeur), texte `text_muted` ; réduite : texte
  `disabled`, icône à 45 %.
- Anneau d'accent de 2 px, à l'intérieur de la capsule.
- Boutons : 20 px, survol `button_hover`, `close_hover` pour fermer.

## Cas limites

- **Emplacement étroit** avec beaucoup de fenêtres : les capsules se
  réduisent à leur icône ; au-delà, les dernières sont coupées par le bord
  du groupe. Pas de défilement.
- **Appli X11 qui change d'avis** sur sa décoration : la barre du shell
  apparaît ou disparaît, la capsule ne bouge pas.
- **Glisser en cours** : la capsule de la fenêtre glissée reste à sa place
  jusqu'au relâchement.

## Pas prévu

- Réordonner les capsules d'un groupe en les glissant.
- Capsules pour les fenêtres flottantes.
- Vignettes au survol.
- Distinguer deux fenêtres de la même appli (elles ont la même capsule).
