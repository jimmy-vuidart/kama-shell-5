# L'or devient l'accent unique de coquille

Prompt de travail pour Claude Code. Lire `CLAUDE.md` d'abord : ses conventions
(français avec Jimmy, code en anglais, CI locale verte, commit sur `main` à la
fin) valent ici.

## La décision

1. **L'accent de coquille est un or métallique**, le même partout, en thème
   clair comme en thème sombre. Ce n'est plus une couleur unie mais un métal :
   des dégradés verticaux éclairés d'en haut, du crème au brun.
2. **La couleur d'accent réglable disparaît.** Plus de ligne `accent` dans la
   configuration, plus de choix dans l'écran « Paramètres », plus d'accent
   transmis entre les programmes. Le bleu d'origine (`#2f6fed`, `#7aa7ff`)
   disparaît avec elle.
3. **Les gris se réchauffent** : l'ardoise bleutée du thème sombre devient un
   noir chaud, le blanc du thème clair un ivoire.

Ce rendu a été choisi par Jimmy après une longue série de maquettes. Ne pas le
réinterpréter : pas d'or uni « jaune », pas de reflets multiples, pas de
lumière venant d'un coin, pas de texture. Le fond d'écran, les icônes des
applis, le rouge de danger et le vert de niveau du mixeur ne changent pas.

## Les images

Dans ce dossier, à ouvrir avant d'écrire du code :

| Fichier | Ce qu'il montre |
|---|---|
| `maquette-sombre.png`, `maquette-clair.png` | La scène entière, à l'échelle 2 |
| `detail-controle.png` | Le tiroir : recherche, bascules en lingot, curseur |
| `detail-dock.png` | La barre : lanceur, repères, séparateurs, heure |
| `detail-capsules.png` | Le groupe de capsules et le haut de la fenêtre active |
| `detail-anneau.png` | La coupure nette de l'anneau, à mi-hauteur |
| `maquette-sombre.html`, `maquette-clair.html` | Les mêmes scènes en HTML : **les valeurs CSS y font foi** |

Les gros plans sont à l'échelle 3, sombre à gauche, clair à droite.

La scène est une maquette simplifiée : icônes de remplacement, tiroir réduit à
un extrait, contenu de fenêtre factice. **Seuls les couleurs, les dégradés et
les filets font foi**, pas la disposition ni les dimensions, qui restent
celles du code et de `docs/maquettes/`.

## La palette

Onze tons, du plus clair au plus sombre. Les noms sont ceux de ce document ;
les nommer en anglais dans le code.

| Ton | Sombre | Clair | Rôle |
|---|---|---|---|
| éclat | `#fffbe8` | `#f6e7b0` | sommet de l'anneau |
| clair | `#f6e3a1` | `#e3c878` | haut de toutes les pièces |
| or | `#dfb85a` | `#c29a38` | corps des pièces |
| ocre | `#b0832b` | `#98721f` | bas des pièces |
| bronze | `#8a5e1c` | `#7a5416` | pied des lingots et des barres |
| brun | `#6b4413` | `#5e3c10` | pied des pièces fines |
| ombre | `#2b1805` | `#2b1805` | coupure de l'anneau |
| moyen | `#a87c2a` | `#94701f` | bas de l'anneau |
| retour | `#cfa64c` | `#b8902f` | pied de l'anneau |
| texte or | `#e6c680` | `#77560f` | heure, intitulés |
| encre | `#2a1e04` | `#2a1e04` | texte posé sur l'or |

Le thème clair est volontairement plus sombre d'un cran : les tons les plus
clairs se confondraient avec l'ivoire.

## Les quatre recettes

Tous les dégradés sont **verticaux**, clair en haut, mesurés sur la hauteur de
la pièce. Les positions sont en pourcentage de cette hauteur.

**Anneau** (fenêtre active, 2 px, mesuré sur la hauteur de la fenêtre) :

```
éclat 0 % · clair 2 % · or 14 % · ocre 45 % · ombre 45,3 % · brun 62 % · moyen 84 % · retour 100 %
```

Le passage d'ocre à ombre entre 45 % et 45,3 % est une coupure nette, voulue :
c'est elle qui fait le métal (voir `detail-anneau.png`). Le bord haut est donc
clair d'un bout à l'autre, les deux côtés portent la coupure à la même hauteur,
le bord bas est d'un or moyen.

**Pièce fine** (tour de la capsule active, repère d'une appli ouverte dans le
dock, carrés du lanceur) :

```
clair 0 % · or 35 % · ocre 65 % · brun 100 %
```

Les deux carrés du bas du lanceur sont à 78 % d'opacité, ceux du haut à 100 %.

**Barre** (partie remplie d'un curseur, trait sous le champ de recherche) :

```
clair 0 % · or 40 % · bronze 100 %
```

**Lingot** (bascule allumée, bouton « Par appli », et tout aplat d'accent qui
porte du texte) :

```
clair 0 % · or 22 % · ocre 64 % · bronze 100 %
```

avec un biseau par-dessus : arête haute de 1 px `rgba(255,251,232,.9)`, pied de
2 px `rgba(43,24,5,.5)`, flanc gauche de 1 px `rgba(255,240,190,.22)`, flanc
droit de 1 px `rgba(43,24,5,.25)`. Le texte est en encre, le sous-texte en
encre à 86 %.

## Ce qui reste à plat

- **Filets** du dock, des groupes de capsules et du tiroir : un contour de
  1 px, et par-dessus une arête de 1 px sur le bord haut seulement.

  | | Contour | Arête haute |
  |---|---|---|
  | sombre | `rgba(140,98,30,a)` | `rgba(246,227,161,a)` |
  | clair | `rgba(190,160,90,a)` | `rgba(152,114,31,a)` |

  Opacités (contour, arête) : dock 0,5 et 0,7 ; groupe de capsules 0,42 et
  0,55 ; tiroir 0,45 et 0,65.
- **Séparateurs du dock** : `rgba(227,193,90,.3)` en sombre,
  `rgba(150,113,15,.3)` en clair. Le filet sous la barre de titre de la
  fenêtre active prend les mêmes couleurs à 0,45.
- **Heure et intitulés** (« ÉPINGLÉES »…) : texte or.
- **Bouton d'un curseur** : inchangé (clair), cerclé de 1,5 px d'ocre sombre
  `#b0832b`.
- **Ombres portées** : droites, sans décalage horizontal.

## Les gris réchauffés

Valeurs de la maquette, à reporter dans `Palette` (`crates/ui/src/style.rs`) et
dans `Theme` (`crates/compositor/src/theme.rs`) :

| Rôle | Sombre | Clair |
|---|---|---|
| barre du dock | `rgba(28,25,20,.96)` | `rgba(255,253,247,.94)` |
| tiroir, mixeur | `rgba(28,25,20,.98)` | `rgba(250,247,240,.98)` |
| carte, tuile éteinte | `#2f2a21` | `#ffffff` |
| fond de fenêtre | `#171512` | `#fffefb` |
| barre de titre | `#1d1a15` | `#fffdf8` |
| séparateur, piste de curseur | `#2b271f` | `#ebe5d6` |
| capsule affichée, survol de bouton | `#332e24` | `#f1ebdc` |
| texte | `#f4f1ea` | `#241f14` |
| texte atténué | `#a89e88` | `#7a705c` |

Les rôles que la maquette ne montre pas (`backdrop`, `disabled`, `block`,
`hover`, `scrim`…) sont à déduire dans le même esprit : même clarté
qu'aujourd'hui, teinte tirée vers le brun chaud au lieu du bleu.

## Supprimer la couleur d'accent réglable

Relevé fait le 2026-10-13, à revérifier par une recherche de `accent` :

- **`config/config.toml`** : retirer le commentaire et la ligne `accent`.
- **`crates/config`** : `Settings::accent`, `Change::Accent`, leurs tests
  (`settings.rs`, `edit.rs`).
- **`crates/compositor`** : `Config::accent` et `Config::accent()`
  (`config.rs`), `Options::accent` et `State::accent` (`state.rs`, `main.rs`),
  `restyle` et l'accent du message d'état (`ipc.rs`, `reload.rs`),
  `Theme::with_accent` (`theme.rs`), les champs `accent` des descriptions de
  décoration (`decoration/paint.rs`, `shell.rs`, `placement.rs`).
- **`crates/ipc`** : le champ `accent` de l'état poussé, et ses exemples.
- **`crates/dock`** : `Config::accent` (`config.rs`), la palette construite
  avec l'accent (`app.rs`, `backend/mock.rs`).
- **`crates/settings`** : la liste `ACCENTS` (`model.rs`), le texte saisi
  (`app.rs`), la carte « Couleur d'accent » et ses pastilles (`view.rs`) ; la
  page Apparence ne garde que le thème, et sa description change.
- **`crates/ui`** : `Palette::new` ne prend plus d'accent ; `parse_color`
  s'en va s'il ne sert plus à rien.
- **Documentation** : `CLAUDE.md` (apparence, décisions techniques, réglages
  de `config.toml`, écran « Paramètres »), `crates/settings/README.md`,
  `crates/dock/README.md`, `docs/tasks/PHASE_4.md`.

**Point à ne pas rater** : le compositeur lit la configuration strictement
(`deny_unknown_fields`). Un `config.toml` existant qui contient encore
`accent = "…"` ne doit pas empêcher coquille de démarrer ni de relire sa
configuration : la ligne doit être ignorée avec un avertissement dans le
journal. Le prouver par un test.

## Où chaque moteur de rendu doit changer

Vérifier l'API des versions réellement utilisées avant de coder (règle de
`CLAUDE.md`).

- **Anneau de la fenêtre active** : `rounded.frag` ne connaît qu'une couleur
  (`crates/compositor/src/render/shaders/`, appelé depuis `render/scene.rs`).
  Il lui faut le dégradé à huit arrêts, mesuré sur la hauteur de la forme
  (`shape_size.y`), sans casser le remplissage uni dont se servent les fonds
  de fenêtre.
- **Capsules et barres de titre** (`decoration/paint.rs`, tiny-skia) : le
  tour de la capsule active passe d'un trait uni à la recette « pièce fine ».
- **Emplacements de dépôt pendant un glisser** (`input.rs`, `overlay`) : non
  maquettés. Garder les opacités actuelles avec le ton « or » à plat.
- **Dock et écran « Paramètres »** (iced) : les fonds acceptent un dégradé
  linéaire, pas les bordures ni les ombres intérieures. Le biseau des lingots
  et l'arête haute des filets demandent donc une construction à part (lignes
  de 1 px, conteneurs emboîtés…).

Pour tout ce que la maquette ne montre pas (menu contextuel du dock, mixeur,
pagination du tiroir, avatar, listes et champs de l'écran « Paramètres », barre
de rangement), appliquer la même règle :

| Aujourd'hui | Devient |
|---|---|
| aplat d'accent portant du texte ou une icône | lingot, contenu en encre |
| trait ou petit repère d'accent | pièce fine, ou barre s'il est horizontal |
| texte en couleur d'accent | texte or |
| accent en transparence (survol, sélection) | ton « or » à la même opacité |

`Palette::on_accent()` devient l'encre dans les deux thèmes.

## Déroulé et vérification

Trois étapes, avec un arrêt après chacune pour que Jimmy regarde :

1. Supprimer l'accent réglable et poser la palette et les gris réchauffés, l'or
   encore à plat (ton « or »). Tout compile, la CI est verte.
2. Le métal dans le compositeur : anneau, capsules.
3. Le métal dans le dock et l'écran « Paramètres ».

À chaque étape : `scripts/ci.sh`, puis des captures en imbriqué
(`--screenshot`, `--debug-input`) en clair et en sombre, relues à côté des
images de ce dossier. Le test `accents_match_the_design` de `crates/ui` et
ceux de `crates/compositor/src/theme.rs` sont à réécrire pour la nouvelle
palette. Mettre `CLAUDE.md` à jour à la fin.
