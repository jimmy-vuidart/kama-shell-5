# 03 — Ombres douces

- **Priorité :** deuxième vague, après **01**, et après **04** (même fichier `render/scene.rs`)
- **Nature :** évolution du rendu
- **Zone :** `crates/compositor/src/render/` (nouveaux `shadow.rs` et `shaders/shadow.frag`), `render/scene.rs`, `render/shaders.rs`
- **Se vérifie :** captures en imbriqué
- **Certitude :** lu dans le code

Lire d'abord [le README de la refonte](README.md) : la **règle cardinale**
(aucune disposition ne change) et le jeton `shadow`.

## But

Des ombres douces, teintées (jeton `shadow` du ticket 01), sous ce que le
compositeur dessine et qui flotte au-dessus du bureau. Aujourd'hui il n'y en a
aucune : le compositeur ne dessine pas d'ombre, seuls les programmes iced en
ont (`Palette::shadow()`).

## Piste

- **Shader** `render/shaders/shadow.frag` : ombre gaussienne d'un rectangle
  arrondi. On utilise l'approximation analytique classique, à base de `erf`
  (bord flou d'un rectangle, puis coins par la SDF). Uniformes : taille de la
  forme, rayons, rayon de flou, couleur prémultipliée et décalage. Programme
  ajouté à `Shaders` (`render/shaders.rs`), donc compilé aussi par `--probe`.
- **Élément** : un `PixelShaderElement` comme ceux de `Shape`
  (`render/scene.rs:43-92`), dont l'aire est la forme élargie du rayon de
  flou et du décalage. Il est mis en cache comme `ShapeCache`
  (`scene.rs:58`), pour qu'une ombre inchangée ne cause aucun dégât. Le cache
  vit dans `Chrome` (`scene.rs:96`), à côté de `ring`.
- **Où**, dans `scene::build` (`scene.rs:296-424` pour les fenêtres) :
  - sous chaque **fenêtre décorée** (`!geometry.anchored`), placée **après**
    tous les éléments de la fenêtre dans la liste (elle va du haut vers le
    bas) ;
  - ombre modeste pour une fenêtre rangée (l'espacement est de 8 px, elle ne
    doit pas salir la voisine), plus marquée pour une fenêtre flottante, et
    un peu plus pour la fenêtre active ;
  - sous la **barre de rangement** (`panel`, `scene.rs:468`).
  - Valeurs de départ, à ajuster sur captures :
    - fenêtre rangée : flou 12 px, décalage y 2 px ;
    - fenêtre flottante : flou 28 px, décalage y 8 px ;
    - barre de rangement : flou 18 px, décalage y 4 px.
- **Pas d'ombre** :
  - sur un écran ancré ;
  - sous une fenêtre qui dessine sa propre ombre (applis X11 avec
    `frame_extents`, `xwayland.rs:303-350`) ;
  - sous une appli GTK sans décoration du shell qui dessine déjà la sienne
    autour de sa géométrie.

  Vérifier ces deux derniers cas avec Nautilus et une appli X11.
- **Coût** : l'ombre n'est jamais opaque. Elle ne doit pas empêcher
  l'affichage direct d'un écran ancré, où il n'y en a pas.

## Terminé quand

- Captures en imbriqué, en clair et en sombre :
  - deux fenêtres rangées, une flottante active : ombres visibles, douces,
    teintées, sans bavure sur la voisine ;
  - la barre de rangement a son ombre ;
  - les dispositions sont identiques aux captures d'avant.
- Écran ancré (`kitty --start-as fullscreen`) : aucune ombre sur cet écran.
- Une appli qui dessine sa propre ombre n'en a pas une seconde.
- `scripts/ci.sh` est vert.

## Liens

- Dépend de : 01 (jeton `shadow`).
- Partage `render/scene.rs` avec 04 et 05 : passer après eux.
