# 07 — Paramètres en verre

- **Priorité :** deuxième vague, après **01** ; en parallèle des tickets du compositeur
- **Nature :** apparence
- **Zone :** `crates/settings/src/` (`main.rs`, `app.rs`, `view.rs`)
- **Se vérifie :** captures en imbriqué (le flou n'apparaît qu'avec le ticket 05)
- **Certitude :** lu dans le code ; la transparence de la fenêtre iced reste à confirmer

Lire d'abord [le README de la refonte](README.md) : la **règle cardinale**,
les jetons et la **règle des alphas**.

## But

La fenêtre « Paramètres » devient translucide et laisse voir le bureau
flouté. Barre latérale, cartes et contrôles passent en verre.

**Même disposition partout.** Interdits :
- les carrés pastel dans la barre latérale ;
- les nouvelles sections ;
- un réglage du flou ;
- les tailles changées.

L'image `references/parametres-sombre.png` donne la matière, pas la
disposition.

## Constat

- Fenêtre iced ordinaire de 900×640 (`main.rs:53-73`), `app_id`
  `coquille-settings` (`main.rs:20`). Elle est décorée par le compositeur
  (barre de titre de 38 px) comme toute fenêtre.
- Le fond est opaque : `App::style` (`app.rs:281-287`) rend
  `background_color: view::backdrop(palette)`, soit `alpha(panel, 1.0)`
  (`view.rs:27-29`).
- La barre latérale (`view.rs:36-65`) est un `block` opaque sans rayon. Les
  pages se montrent par `page_button` (`view.rs:67-105`).
- Les éléments communs sont dans `view.rs:985-1194` : `section`, `card`
  (rayon 10), `setting`, `switch`, `segmented`, `picker`, `slider_style`.
- Les aperçus de thème (`theme_tile`, `view.rs:309-373`) dessinent un
  mini-écran dans la palette de chaque thème. Ils suivront la nouvelle palette
  d'eux-mêmes.

## Piste

1. **Fenêtre transparente** :
   - `window::Settings { transparent: true, .. }` (`main.rs`), champ présent
     dans iced 0.14 (`iced_core/src/window/settings.rs:69`) ;
   - `App::style` rend un fond **translucide** : `glass`, ou `Color::TRANSPARENT`
     si la barre latérale et le contenu portent chacun leur verre. Choisir ce
     qui garde le contenu lisible, en clair comme en sombre ;
   - les coins sont rognés par le compositeur ; rien à faire côté client.
2. **Barre latérale** (`view.rs:36-65`) : `glass` au lieu de `block`, avec un
   filet `separator` vers le contenu. `page_button` : la page courante sur
   `card`, le survol en `hover`. Le petit trait d'accent ne change pas.
3. **Contenu** : `card` et `section` (`view.rs:985-1010`) en `card`
   translucide avec un liseré `rim`, mêmes rayons. `segmented`, `picker`,
   `switch` et `slider_style` prennent les jetons ; les choix actifs restent
   pleins d'accent.
4. **Thème iced de repli** (`app.rs:290-295`, `iced::Theme::Light` /
   `Dark`) : vérifier que les barres de défilement restent lisibles sur le
   verre.
5. **Règle des alphas** : verre au-dessus de 0.45, sinon le ticket 05 ne le
   floute pas.
6. **Hors de coquille** (la fenêtre lancée sous une autre session) : une
   fenêtre transparente sans flou peut être illisible. Quand
   `coquille_ipc::socket_from_env()` ne trouve rien (aucun compositeur
   coquille, voir `app.rs:271`), garder un fond opaque `glass_strong`.

## Terminé quand

- Captures en imbriqué (`target/debug/coquille-settings --config FICHIER --page appearance`, lancé dans coquille), en clair et en sombre, pour chaque page (`appearance`, `screens`, `game`, `system`…), avant et après, superposables à la matière près.
- Avec le ticket 05 fait : le bureau est flouté derrière la fenêtre, et le
  texte reste lisible.
- Lancée hors de coquille : fond opaque, lisible.
- Le test qui vérifie que le compositeur accepte tout ce qu'écrit l'écran
  « Paramètres » passe toujours.
- `scripts/ci.sh` est vert.

## Liens

- Dépend de : 01. Le flou devient visible avec 05.
- En parallèle de : 03, 04, 05, 06.
