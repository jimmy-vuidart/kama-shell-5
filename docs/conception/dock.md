# Dock

Le dock remplace les barres latérales et le lanceur des maquettes d'origine.
Un seul programme (`coquille-dock`), plusieurs surfaces layer-shell, un seul
état : la **barre**, le **tiroir** d'applications qu'elle ouvre, le **centre
de contrôle** en pied de tiroir et le **mixeur** par application.

Ce document garde l'intention de départ (12 octobre 2026) et la façon de lire
les maquettes. Ce que fait le dock aujourd'hui, ses options et ses limites
sont dans `crates/dock/README.md` ; les règles d'apparence communes
(verre, liserés, ombres, alphas) dans `CLAUDE.md`. **Le code fait foi** pour
les mesures.

## Lire les maquettes (`docs/maquettes/dock/`)

- `07-dock.png` : la barre au repos, au survol, au clic droit ;
  `04-soiree-raid.png` : la barre en situation, à l'échelle réelle ;
  `08-tiroir.png` : le tiroir, son centre de contrôle et le mixeur.
- `07` montre l'écran gauche **agrandi ×1,5** : diviser ses mesures par 1,5.
- `04` montre encore une barre sur le bord droit de l'écran droit : elle est
  abandonnée sous cette forme (chaque écran peut aujourd'hui avoir sa barre,
  réglée dans `[dock.screens]`).
- Les carrés colorés sont remplacés par les vraies icônes (thème Reversal) ;
  la section « SUR CET ÉCRAN » est devenue « OUVERTES », qui montre les
  fenêtres des trois écrans ; les textes réels sont plus grands que ceux des
  maquettes.

## Barre (verticale, mesures d'origine)

- Largeur **44**, à **8** du bord, centrée verticalement, rayon **14** ;
  zone exclusive de **60 px**. Barre, marge et zone suivent la taille des
  icônes (`[dock] icon_size`, 24 par défaut).
- Emplacement d'icône **36 × 36** (rayon 10). Indicateurs : barre d'accent
  = ouverte sur l'écran du dock ; point gris = ouverte ailleurs. Badge
  non-lus rouge `#e5484d`.
- Heure en bas sur deux lignes, en gras. Au survol, la barre se déploie
  (~200) : noms, sous-titres, titres « FAVORIS » / « OUVERTES », heure
  complète et date.
- Menu contextuel (~240) : « OUVRIR SUR » Gauche / Centre / Droite, Nouvelle
  fenêtre, Retirer des favoris (ou Épingler), Oublier sa place, Fermer la
  fenêtre (en rouge, si elle est ouverte).

## Barre horizontale (verre liquide)

- **Un seul rythme** : un quart de tuile (au moins 6 px) au-dessus et
  au-dessous des tuiles, entre deux tuiles, aux bouts, et de chaque côté
  d'un séparateur (un trait haut d'une demi-tuile). À 24 px : barre de
  **36**, à 8 px du bord, zone exclusive de **52**. Coins concentriques à
  ceux des tuiles (rayon de tuile + un rythme).
- **Verre liquide** : le dock donne au compositeur la forme de la barre
  (`set_glass`) ; celui-ci y réfracte le fond (bord bombé, réfraction,
  dispersion légère) et l'éclaire d'un liseré spéculaire. Le dock n'y pose
  qu'un voile très léger (`glass_clear`) et son ombre.
- **Tuiles uniformes** : les icônes Reversal couvrent leur tuile ; les
  autres (hicolor, pixmaps) sont posées, plus petites, sur une plaque
  claire au même arrondi ; une jaquette la remplit. Ombre discrète, levée
  au survol avec un voile clair.
- Les notifications ont la même épaisseur, le même verre et le même
  arrondi.

## Tiroir

- Glisse depuis le bord de la barre, **sur toute la hauteur** de l'écran,
  largeur **640** par défaut (`[drawer] width`), coins extérieurs arrondis
  (~18), le reste de l'écran voilé (`scrim`). La barre reste visible, son
  bouton du lanceur en accent.
- De haut en bas : **recherche** ; **Épinglées** (une rangée, l'écran
  d'ouverture sous chaque icône) ; **Applications** (grille 6 × 3, de A à
  Z, paginée) ; **Jeux** (une rangée de jaquettes Steam, lien « Toute la
  ludothèque »). **Pas de section « récents ».**
- Pied, en deux moitiés (notifications à gauche, centre de contrôle à
  droite, quel que soit le côté de la barre), de la même hauteur :
  - **Notifications** (21 octobre 2026 : sorties du centre de contrôle) :
    une carte par notification encore ouverte, la plus récente en haut
    (icône, résumé, appli et ancienneté, deux lignes du corps, actions,
    croix), « Tout effacer », défilement ; une cloche quand il n'y en a pas ;
  - **centre de contrôle**, dans une carte (`card`), le même dans les deux
    lanceurs : cinq bascules deux par deux (Réseau, Bluetooth, Ne pas
    déranger, Éclairage nocturne, Thème sombre ; activée = fond d'accent),
    curseurs Volume (une flèche ouvre le mixeur) et Luminosité (« 3
    écrans ») l'un sous l'autre, puis l'utilisateur (avatar, nom, batterie)
    sur sa ligne, et dessous Capture, Paramètres, Se déconnecter, Veille,
    Redémarrer, Éteindre (en rouge) sur toute la largeur.

Le lanceur d'une barre horizontale est lui aussi un tiroir, **sur toute la
largeur** de l'écran (comme le vertical est sur toute sa hauteur), qui monte
du bord bas avec la barre dans son pied : recherche, puis quatre colonnes,
Épinglées et Applications (seules à s'élargir avec l'écran), Jeux (cédés en
premier sur un écran étroit), Notifications et centre de contrôle (340 px).
Sur un écran très large, les colonnes restent groupées au milieu (1900 px au
plus). Le mixeur s'ouvre au-dessus du centre de contrôle.

## Mixeur

Panneau (~290) à côté du tiroir, pointant vers la flèche du volume : sortie
audio (seul endroit où la choisir), volume général, une ligne par appli
(icône, nom, état, curseur, niveau en direct en vert, bouton « couper » rouge
quand le son est coupé), note sur la mémorisation des volumes. Chaque appli
retrouve son volume à sa prochaine ouverture.
