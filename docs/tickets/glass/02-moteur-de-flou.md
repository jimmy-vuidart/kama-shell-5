# 02 — Moteur de flou : `GlassElement`

- **Priorité :** première vague (sans dépendance)
- **Nature :** évolution du rendu
- **Zone :** `crates/compositor/src/render/` (nouveaux `glass.rs` et shaders), `crates/compositor/src/config.rs`, `crates/compositor/src/reload.rs`, `config/config.toml`
- **Se vérifie :** tests unitaires, `--probe`, captures en imbriqué
- **Certitude :** API Smithay lue dans le commit épinglé ; le rendu reste à confirmer

Lire d'abord [le README de la refonte](README.md), et surtout « Comment le
flou est possible ».

## But

Un élément de rendu qui dessine **ce qu'il y a derrière lui, flouté**, dans
une forme donnée. Les tickets 04 (décorations) et 05 (nos programmes) le
placent dans la scène. Ce ticket ne place rien de façon définitive : il
fournit l'élément, ses shaders, son réglage et ses tests.

## Ce que fournit Smithay (commit `19c16d3`)

- `Element::is_framebuffer_effect() -> bool` et
  `RenderElement::capture_framebuffer(&self, frame, src, dst, cache: &UserDataMap)`
  (`src/backend/renderer/element/mod.rs:556-606`).
  `capture_framebuffer` est appelé **avant** `draw`, seulement si quelque
  chose a changé derrière l'élément ou s'il signale lui-même un dégât. Le suivi
  des dégâts redessine alors toute la zone derrière lui
  (`src/backend/renderer/damage/mod.rs`, vers la ligne 690).
- `draw` reçoit `cache: Option<&UserDataMap>` : c'est là que l'on garde les
  textures d'une image à l'autre.
- `FrameContext::renderer()` (trait, `src/backend/renderer/mod.rs:372`,
  implémenté pour `GlesFrame`) donne un garde sur le `GlesRenderer`, pour
  créer les textures (`Offscreen::create_buffer`).
- `GlesFrame::with_context(|gl| …)` donne le GL brut **sans** changer la cible
  liée. `GlesRenderer::with_context` la change : à éviter pendant une capture.
- `frame.render_texture_from_to(texture, src, dst, damage, opaque, transform, alpha, program, uniforms)`
  dessine une texture avec un programme de texture personnalisé
  (`compile_custom_texture_shader`). Les uniformes acceptent `_1i`, donc un
  numéro d'unité de texture.

Niri 26.04 (`src/render_helpers/framebuffer_effect.rs`, `blur.rs`) fait la même
chose et montre les pièges : `BlitFramebuffer` est soumis au test de scissor ;
il faut restaurer le framebuffer lié ; il faut rogner `dst` aux bords de la
sortie. **Référence de technique seulement, sous GPL-3 : ne pas copier son
code.**

## Piste

### L'élément

Dans `crates/compositor/src/render/glass.rs` :

- `GlassElement` : `id`, `commit`, `shape: Rectangle<i32, Physical>`, `mask` et
  `strength`.
  - Sa **géométrie** est `shape` élargie de la **marge du flou**, qui dépend de
    `strength`, puis rognée à la sortie. Ainsi les pixels qu'il lit autour de
    sa forme sont redessinés à chaque capture, et non repris d'une image
    précédente.
  - `is_framebuffer_effect()` rend `true`, `opaque_regions` rend vide, et
    `kind()` rend `Kind::Unspecified` : jamais d'affichage direct.
- **Un état par élément** qui dure entre les images, pour garder l'`Id` et le
  `CommitCounter`, sur le modèle de `ShapeCache` (`render/scene.rs:58`).
  Seules la forme et le masque qui changent incrémentent le commit.
- **`Mask`** :
  - `Rounded { radii: [f32; 4] }` : la SDF de rectangle arrondi de
    `render/shaders/rounded.frag`. Pour les décorations du shell.
  - `Alpha { texture: GlesTexture, src, dst }` : la couverture vaut
    `smoothstep(0.2, 0.4, a)`, où `a` est l'alpha de la texture d'une surface
    cliente (`WaylandSurfaceRenderElement::texture()`, qui rend une
    `WaylandSurfaceTexture`). Pour nos programmes (ticket 05). Seule une
    surface en `Transform::Normal`, à l'échelle 1, est prise en charge ;
    sinon, pas de verre (avec un `debug!`).

### La capture et le flou

Dans `capture_framebuffer` :

1. Rogner `dst` à la sortie.
2. Copier la zone de la cible courante vers une `GlesTexture` en cache
   (`UserDataMap`), par `BlitFramebuffer`. Désactiver le scissor, puis
   restaurer le framebuffer et le scissor.
3. Flouter par **Kawase double** :
   - N passes de réduction (`blur_down.frag`) puis N d'agrandissement
     (`blur_up.frag`), N et le décalage venant de `strength` ;
   - les textures intermédiaires restent en cache et ne sont réallouées que
     si la taille change.

### Le dessin

Dans `draw` :

- la texture floutée est dessinée par `glass.frag`, qui :
  - applique le masque ;
  - rehausse légèrement la saturation (≈ 1.15) ;
  - ajoute un **grain fin** contre les bandes ;
- avec `Mask::Alpha`, la texture de la surface est liée à l'unité 1 par
  `frame.with_context`, puis l'unité 0 est restaurée ;
- le passage des coordonnées du fragment à celles de la forme reprend
  `render/clip.rs` (`frag_to_local`) et `render/shaders/clip.frag`, qui le font
  déjà pour le rognage des coins.

### Le reste

- **`unsafe`** : les appels GL bruts sont `unsafe`. C'est une nécessité, à
  documenter par un commentaire (règle de `CLAUDE.md`) : pourquoi, et quel
  état GL est restauré.
- **Shaders** : `Shaders` (`render/shaders.rs:7-38`) reçoit les nouveaux
  programmes, compilés dans `Shaders::compile`. Les passes de flou, qui ne
  dessinent pas d'élément, peuvent compiler leur propre programme GL comme
  le fait niri, ou passer par `compile_custom_texture_shader` si cela suffit.
  `--probe` construit déjà `Shaders` (`backend/udev.rs:135`) : les nouveaux
  shaders y sont donc compilés sur la vraie carte.
- **Configuration**, dans `crates/compositor/src/config.rs` :
  - une section `[glass]` (`#[serde(default, deny_unknown_fields)]`, comme les
    autres) : `blur = true` et `strength = 3` (de 1 à 5, borné à la lecture) ;
  - `GlassConfig` gardé dans `State` ;
  - `reload.rs` l'applique à chaud, puis force un redessin complet ;
  - avec `blur = false`, les tickets 04 et 05 ne placent aucun `GlassElement`.
    Les surfaces restent translucides, sans flou ; le jeton `glass_strong`
    sert de couleur de repli, choisie par ces tickets.
- **`config/config.toml`** : la section, commentée, sous les autres, avec son
  explication en français. La crate `crates/config` (réécriture par l'écran
  « Paramètres ») n'est **pas** touchée : aucun réglage à l'écran.
- **Mode jeu** : ce ticket ne place rien ; les tickets 04 et 05 garantissent
  qu'aucun `GlassElement` n'est créé sur un écran ancré.

## Terminé quand

- Tests unitaires de la partie pure :
  - marge selon `strength` ;
  - élargissement et rognage de la géométrie aux bords de la sortie ;
  - bornes de `strength` ;
  - lecture de `[glass]` (valeurs par défaut, section absente, valeur hors
    bornes).
- `cargo run -p coquille -- --probe` compile les nouveaux shaders sur la
  NVIDIA, sans erreur.
- Essai en imbriqué, avec un `GlassElement` placé **provisoirement, sans le
  commiter**, sous la barre de rangement ou à une place fixe, au-dessus d'une
  fenêtre kitty qui défile. Captures (`--screenshot`) en clair et en sombre :
  - le flou est visible ;
  - les bords sont nets selon le masque ;
  - rien ne « traîne » quand le contenu derrière change ;
  - aucune bavure du curseur ;
  - rien ne change hors de la forme.
- La capture d'écran du compositeur (`screenshot.rs:85`, rendu hors écran)
  fonctionne toujours avec un `GlassElement` dans la scène.
- `scripts/ci.sh` est vert.

## Liens

- Débloque : 04, 05.
- Hors périmètre : le protocole `ext-background-effect` (voir le README).
