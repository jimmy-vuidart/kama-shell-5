# 06 — Dock en verre

- **Priorité :** deuxième vague, après **01** ; en parallèle des tickets du compositeur
- **Nature :** apparence
- **Zone :** `crates/dock/src/ui/` (`bar.rs`, `drawer.rs`, `menu.rs`, `tray_menu.rs`), `crates/ui/src/style.rs`
- **Se vérifie :** captures en imbriqué (le flou n'apparaît qu'avec le ticket 05)
- **Certitude :** lu dans le code

Lire d'abord [le README de la refonte](README.md) : la **règle cardinale**,
les jetons et la **règle des alphas**.

## But

La barre, le tiroir, le centre de contrôle, le mixeur, les menus, la zone de
notification et les infobulles passent en verre.

**Mêmes widgets, mêmes places, mêmes tailles, mêmes rayons, mêmes textes.**
Interdits :
- les cartes à la place du centre de contrôle ;
- les boutons ronds ;
- les rayons plus grands ;
- les icônes déplacées.

Les images de `references/` donnent la matière, pas la disposition.

## Constat

- Le dock prend toutes ses couleurs dans `coquille_ui::style::Palette`, par
  `dock.palette()` (`crates/dock/src/app.rs:361`). Après le ticket 01, les
  champs existants (`bar`, `bar_border`, `panel`, `block`, `card`…) sont déjà
  alimentés par les jetons de verre : **une bonne partie du travail est faite
  par le ticket 01.**
- Les surfaces layer-shell sont déjà transparentes
  (`background_color: Color::TRANSPARENT`, `app.rs:1647`).
- Certaines surfaces **forcent l'opacité** et ne suivent donc pas la palette :
  - infobulles de la barre : `a: 1.0` (`bar.rs:513`, `:566`) ;
  - mixeur : `a: 1.0` (`drawer.rs:1194`) ;
  - menu de l'appli : `a: 1.0` (`menu.rs:102`) ;
  - menu de la zone de notification : `a: 0.96` (`tray_menu.rs:102`).
- Couleurs codées en dur :
  - poignée de redimensionnement `alpha(WHITE, 0.55)` (`drawer.rs:75`,
    `:1019`) ;
  - fond du bouton « jouer » des jaquettes (`drawer.rs:453-456`) ;
  - bouton des curseurs `WHITE` (`drawer.rs:567`).

## Piste

1. **Barre** (`bar.rs:620-625` couchée, `:936-937` debout) :
   `outlined(palette.glass, palette.rim, metrics.radius)` et ombre douce
   (`palette.shadow()`). Rayon inchangé.
2. **Tiroir** :
   - panneau (`drawer.rs:142-153`) et panneau au-dessus d'une barre couchée
     (`drawer.rs:1005-1012`) en `glass`, avec un liseré `rim` (mêmes rayons) ;
   - scrim (`drawer.rs:36-39`, `:976`) : `palette.scrim`, **sous 0.18
     d'alpha**, pour ne pas être flouté par le ticket 05 ;
   - champ de recherche (`drawer.rs:174-200`) en `card`, bordure d'accent
     inchangée.
3. **Centre de contrôle**, même disposition :
   - le bloc (`drawer.rs:794-796`) en `card` translucide ;
   - les tuiles éteintes (`drawer.rs:584-620`, `:595`) en `glass_strong` avec
     un liseré `rim` ; les tuiles allumées restent pleines d'accent ;
   - les curseurs prennent la piste `separator` et le remplissage d'accent,
     avec un bouton en `glass_strong` ou blanc selon le thème.
4. **Mixeur** (`drawer.rs:1185-1199`) : on enlève `a: 1.0`, fond en
   `glass_strong` avec un liseré `rim`. Le bloc intérieur (`drawer.rs:1109`)
   en `card`. Sélecteur de sortie audio (`drawer.rs:1048-1076`) en `card`,
   avec ses bordures actuelles.
5. **Menus et infobulles** (`menu.rs:100-106`, `tray_menu.rs:100-106`,
   `bar.rs:507-519`, `:560-571`) : on enlève les opacités forcées, fond en
   `glass_strong`, liseré `rim`.
6. **Couleurs en dur** : elles passent par la palette (`rim`, `glass_strong`).
   La couleur des icônes d'applis et des pastilles de repli ne change pas.
7. **Règle des alphas** : toute ombre ou tout voile reste sous 0.18, tout
   verre au-dessus de 0.45. Si une couleur de `Palette` manque, on l'ajoute à
   la crate de jetons (ticket 01), jamais dans le dock.
8. **Lisibilité** : en clair, le verre blanc à 0.55 sur un fond pastel garde
   le texte `#1d2033` lisible ; en sombre, vérifier `muted` sur l'indigo. On
   ajuste les jetons (dans la crate), pas le dock.

## Terminé quand

- Captures en imbriqué (`target/debug/coquille --role left --spawn "$PWD/target/debug/coquille-dock --mock"`, voir `crates/dock/README.md`), en clair et en sombre, avant et après, superposables à la matière près :
  - barre au repos, barre déployée au survol ;
  - tiroir ouvert, avec le centre de contrôle ;
  - mixeur ;
  - menu de clic droit sur une appli ;
  - menu de la zone de notification ;
  - barre horizontale (`[dock.screens]`) ;
  - infobulle.
- Avec le ticket 05 fait : le fond est flouté sous chaque surface de verre, et
  pas sous le scrim.
- `scripts/ci.sh` est vert.

## Liens

- Dépend de : 01. Le flou devient visible avec 05.
- En parallèle de : 03, 04, 05, 07.
