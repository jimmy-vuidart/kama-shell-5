# 05 — Verre derrière nos programmes

- **Priorité :** deuxième vague, après **02** ; premier des trois tickets qui touchent `render/scene.rs`
- **Nature :** évolution du rendu
- **Zone :** `crates/compositor/src/render/scene.rs`, `crates/compositor/src/panels.rs`
- **Se vérifie :** captures en imbriqué
- **Certitude :** lu dans le code ; le rendu du masque alpha reste à confirmer

Lire d'abord [le README de la refonte](README.md) : « Comment le flou est
possible » et la **règle des alphas**.

## But

Le dock, son tiroir, leurs menus et la fenêtre « Paramètres » sont dessinés
par nos programmes iced. Ils laissent voir ce qui est derrière eux, **flouté**.
Ces programmes ne savent pas demander le flou (pas d'`ext-background-effect`
dans `iced_layershell`) : c'est le compositeur qui le leur donne. **La forme
du flou suit l'alpha de leurs pixels** (`Mask::Alpha` du ticket 02).

## Constat

- Les panneaux layer-shell sont dessinés par `panel_elements`
  (`render/scene.rs:154-192`) : menus d'abord, puis la surface, sans rien
  derrière, avec une opacité fixe de 1.0.
- `Panel` (`panels.rs:18`) a `surface: LayerSurface`, `layer`, `output` et
  `origin`. Son espace de noms se lit par `panel.surface.namespace()` (déjà
  utilisé par `screenshot.rs:103`). Le dock emploie `coquille-dock` et
  `coquille-drawer` (`crates/dock/src/app.rs:468`, `:763`).
- Un écran ancré n'a aucun panneau (`panels.rs:56-87`) : rien à faire pour le
  mode jeu côté panneaux.
- La fenêtre « Paramètres » est une fenêtre ordinaire, d'`app_id`
  `coquille-settings` (`crates/settings/src/main.rs:20`). Le compositeur
  connaît l'`app_id` de chaque fenêtre (`ManagedWindow::app_id`,
  `shell.rs:46`). Ses surfaces sont rognées aux coins par `ClippedSurface`
  (`scene.rs:379-389`).

## Piste

1. **Panneaux.** Dans `panel_elements`, pour chaque panneau dont l'espace de
   noms commence par `coquille-` : pour la surface principale et pour chaque
   menu (popup), on prend les éléments rendus et, pour ceux dont la texture
   est une vraie texture (`WaylandSurfaceRenderElement::texture()`), on ajoute
   un `GlassElement` en `Mask::Alpha`, placé **juste après** la surface dans la
   liste (donc dessiné juste avant elle).
2. **Fenêtre « Paramètres ».** Dans la boucle des fenêtres (`scene.rs:296-424`),
   une fenêtre dont l'`app_id` vaut `coquille-settings` reçoit le même verre.
   Il est en plus rogné aux coins de la fenêtre (mêmes `radii` que
   `ClippedSurface`), et placé après ses surfaces, avant les formes de fond
   (`beside`, `below`).
3. **État entre images.** Un `GlassElement` par surface, gardé d'une image à
   l'autre : dans `Panel` (ou une table de la scène indexée par surface) pour
   les panneaux, dans `Chrome` (`scene.rs:96`) pour la fenêtre. Un nouvel `Id`
   à chaque image ferait tout recapturer.
4. **Masque** : la couverture vaut `smoothstep(0.2, 0.4, a)` (ticket 02). Le
   scrim du tiroir et les ombres iced restent sous 0.18 d'alpha (ticket 01) :
   ils ne sont **pas** floutés. Les surfaces de verre, au-dessus de 0.45, le
   sont.
5. **`[glass] blur = false`** : rien n'est ajouté.
6. **Les autres programmes** (panneaux d'autres espaces de noms, autres
   fenêtres) ne changent pas.
7. **Coût.** Le tiroir est une surface plein écran, mais seuls ses pixels de
   verre sont floutés. Le flou n'est recalculé que si ce qui est derrière
   change, grâce au suivi des dégâts de Smithay. Le vérifier avec une vidéo ou
   un terminal qui défile derrière le tiroir ouvert, et noter le temps d'image
   (`RUST_LOG=debug` ou traces existantes).

Avant les tickets 06 et 07, les surfaces du dock et des Paramètres sont encore
presque opaques : le flou ne s'y voit qu'à peine. Pour vérifier ce ticket, on
peut baisser provisoirement l'opacité d'une couleur de `Palette`, **sans la
commiter**. Si 06 et 07 sont déjà faits, il n'y a rien à faire.

## Terminé quand

- Captures en imbriqué, en clair et en sombre, avec kitty qui affiche du
  texte **derrière** :
  - la barre du dock, au repos et déployée au survol ;
  - le tiroir ouvert ;
  - un menu du dock (clic droit) ;
  - le mixeur ;
  - la fenêtre « Paramètres ».

  Le texte de kitty est flouté exactement sous les surfaces de verre, pas
  sous le scrim ni sous les ombres, et rien ne déborde des bords arrondis.
- Une barre qui se déploie au survol : le flou suit, image par image, sans
  retard visible (`--debug-input` avec des `move`).
- Un panneau d'un autre programme n'est pas flouté.
- `scripts/ci.sh` est vert.

## Liens

- Dépend de : 02.
- Rend visibles : 06, 07.
- Partage `render/scene.rs` avec 03 et 04 : passer en premier.
