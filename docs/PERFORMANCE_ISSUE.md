# Problèmes de performance — coquille

Audit statique complet du dépôt (`compositor`, `dock`, `apps`, `model`, `ipc`, `settings`, `polkit`, `keyring`, `ui`, `vendor/smithay`, `vendor/iced_wgpu`) réalisé sur le commit `5331cc8`.

Cibles matérielles prises en compte :
- **Bureau** : 3 écrans (`DP-1` 1920×1080, `DP-2` 3440×1440 VRR centre, `HDMI-A-1` 1920×1080), NVIDIA RTX 4070 Ti SUPER, souris haute fréquence, jeux Proton/XWayland.
- **Portable** : 1 écran 4K (`eDP`, 200 %), iGPU Intel, batterie.

Cocher `[x]` au fur et à mesure que les points sont traités.

---

## Lots prioritaires suggérés

- **Lot Jeu / Multi-écrans / VRR** : `#1`, `#2`, `#3`, `#4`, `#14`, `#15`
- **Lot Rendu GPU / 4K Intel** : `#5`, `#10`, `#11`, `#12`, `#13`, `#16`
- **Lot Idle / Batterie / VRAM Dock** : `#7`, `#8`, `#9`, `#18`, `#19`, `#20`, `#21`
- **Lot Réactivité / Stutters CPU & I/O** : `#6`, `#17`, `#22` à `#28`

---

## Critiques

- [ ] **#1 — `[Avéré]` `full_redraw = true` détruit tous les swapchains GBM sur tous les écrans à chaque drag/resize/focus**
  - **Où** : `crates/compositor/src/shell.rs:947-952`, `crates/compositor/src/backend/udev.rs:652-656`, `crates/compositor/src/input.rs:950-1062`
  - **Cause** : `apply_arrangement()` positionne systématiquement `self.full_redraw = true`. Dans `udev.rs`, `full_redraw` appelle `screen.surface.reset_buffers()` sur tous les écrans (même inchangés ou ancrés en jeu), ce qui détruit et réalloue tous les buffers GBM, framebuffers DRM et `EGLImage` (~36 Mo sur 3 écrans, ~33 Mo en 4K). Pendant `Moving`, `Resizing` ou `Splitting`, `apply_arrangement()` est appelé sur chaque `PointerMotion` (1000–8000 Hz, même sans changement de `Rect` entier) et inonde le client de `send_pending_configure()` sans throttling.
  - **Impact** : Saccades au drag/resize/split, pics CPU/noyau/GPU, re-flou intégral des 3 écrans et perturbation possible de l'écran de jeu.
  - **Piste** : Supprimer `full_redraw = true` d'`apply_arrangement()` ; utiliser `reset_buffer_ages()` là où une invalidation est réellement requise ; ajouter un chemin rapide pour `Moving`/`Resizing`/`Splitting` avec court-circuit sur `Rect` identique et cadence des `configure` sur vblank.

- [ ] **#2 — `[Avéré]` `redraw_needed` global + timer `screen.frame` sur frame vide (`is_empty`) cassant le frame pacing multi-écrans & VRR**
  - **Où** : `crates/compositor/src/state.rs:207-209`, `crates/compositor/src/backend/udev.rs:443`, `crates/compositor/src/backend/udev.rs:645-668`, `crates/compositor/src/backend/udev.rs:819-879`, `crates/compositor/src/handlers/compositor.rs:116`
  - **Cause** : `redraw_needed` est un booléen unique pour tous les écrans. Un commit sur `DP-1` (vidéo/Discord) ou un mouvement souris réveille `DP-2` (jeu VRR). Si `render_frame` sur `DP-2` renvoie `is_empty = true`, `udev.rs:866-874` passe l'écran en `Pacing::Waiting` avec un `Timer::from_duration(screen.frame)` complet (6,9–16,7 ms) : une frame de jeu committée juste après reste bloquée jusqu'à la fin du timer. De plus, les écrans sont triés par nom (`DP-1` avant `DP-2`), exécutant le flou de l'écran gauche avant de soumettre le jeu.
  - **Impact** : +6 à +16 ms de latence aléatoire et micro-stutters VRR en jeu dès qu'une appli latérale bouge ; réveils et timers continus sur les écrans latéraux inactifs.
  - **Piste** : Suivre `redraw_needed` par `OutputId` ; sur `is_empty == true`, laisser `screen.pacing = Pacing::Idle` (découpler le timer des frame callbacks) ; prioriser les écrans ancrés dans `redraw_screens()`.

- [ ] **#3 — `[Avéré]` Direct scanout bloqué en pratique (`FrameFlags` incomplets, pas de plan curseur ni de tranche `SCANOUT`)**
  - **Où** : `crates/compositor/src/backend/udev.rs:772-780`, `crates/compositor/src/state.rs:591-612`
  - **Cause** : Seul `FrameFlags::ALLOW_PRIMARY_PLANE_SCANOUT` est utilisé (sans `_ANY`), ce qui rejette tout jeu dont le format diffère de `Xrgb8888` (`Argb8888`, `Abgr8888`, `Xbgr8888`, 10-bit). `ALLOW_CURSOR_PLANE_SCANOUT` est absent : dès qu'un jeu affiche un curseur (MMO, RTS, menus), la scène passe à 2 éléments et retombe en composition (et sur le bureau le curseur force une composition primaire à chaque mouvement). `init_dmabuf` ne déclare aucune tranche `TrancheFlags::SCANOUT`.
  - **Impact** : Direct scanout inopérant sur de nombreux jeux Proton/Vulkan et en MMO (+1 frame de latence, composition 3440×1440 / 4K inutile).
  - **Piste** : Activer `ALLOW_PRIMARY_PLANE_SCANOUT_ANY | ALLOW_CURSOR_PLANE_SCANOUT` en mode jeu (et `ALLOW_CURSOR_PLANE_SCANOUT` sur le bureau) ; exposer une tranche `TrancheFlags::SCANOUT` basée sur les formats/modificateurs du plan primaire.

- [ ] **#4 — `[Avéré]` Explosion d'appels `hit_test()` et allocations `panels()` sur chaque `PointerMotion` (jusqu'à 29× en jeu confiné)**
  - **Où** : `crates/compositor/src/input.rs:480-517`, `crates/compositor/src/input.rs:633-663`, `crates/compositor/src/input.rs:817-819`, `crates/compositor/src/input.rs:1617-1620`, `crates/compositor/src/game.rs:39-68`, `crates/compositor/src/panels.rs:128-169`
  - **Cause** : Chaque `PointerMotion` appelle `hit_test()` jusqu'à 3× sur le bureau, 1× même en `Hold::Locked` (FPS), et jusqu'à **29×** en `Hold::Confined` lors d'un edge-scroll (`game::confine()` fait 2×12 itérations de dichotomie). Chaque `hit_test()` appelle `self.panels()` (parcours 4 couches × 3 écrans, `Vec<Panel>`, clones de `LayerSurface`, `String` et `Vec<GlassShape>`). S'ajoutent `Vec<Rect>` (`input.rs:513`) et `separator_at()` appelé même sans `Super`.
  - **Impact** : 12 000 à 232 000 appels `hit_test()` + `panels()` par seconde sur souris 1000–8000 Hz → pics CPU, jitter et latence d'entrée.
  - **Piste** : Parcourir les layer maps sans allouer `Vec<Panel>` ni cloner `liquid` dans `hit_test` ; réutiliser le `Hit` pour `refresh_constraint` ; court-circuiter `Hold::Locked` ; tester la géométrie confinée directement dans `confine()` ; garder `separator_at()` derrière `self.mod_held()`.

- [ ] **#5 — `[Avéré]` Aucune `opaque_regions` dans toute la scène : occlusion culling désactivé et `GL_BLEND` forcé sur 100 % des pixels**
  - **Où** : `crates/compositor/src/render/clipped.rs:75-79`, `crates/compositor/src/render/clipped.rs:117-119`, `crates/compositor/src/render/scene.rs:108`, `crates/compositor/src/render/scene.rs:454-462`, `crates/compositor/src/wallpaper.rs:102-109`
  - **Cause** : `ClippedSurface::opaque_regions` retourne vide et passe `&[]` à `self.inner.draw`. Toutes les fenêtres décorées sont enveloppées dans `ClippedSurface` (même quand `radii == [0.0; 4]`). Le wallpaper et les fonds opaques ne déclarent aucune zone opaque. Smithay ne culle donc jamais le wallpaper, `coquille-home`, les fenêtres masquées ni l'intérieur de `shadow.frag`, et maintient `GL_BLEND` sur 100 % des pixels.
  - **Impact** : Overdraw ≥ 3× et bande passante mémoire GPU multipliée par 2–3× par frame (très coûteux sur iGPU Intel 4K).
  - **Piste** : Intersecter `self.inner.opaque_regions(scale)` avec le rectangle inscrit privé des bandes de coins et le transmettre à `self.inner.draw` ; ne pas utiliser `ClippedSurface` si `radii == [0.0; 4]` ; déclarer le wallpaper opaque.

- [ ] **#6 — `[Avéré]` `Alt+Tab` (`Switcher`) : troncature `fitted()` en O(L²) avec `Shaping::Advanced` + repaint complet à chaque `Tab`**
  - **Où** : `crates/compositor/src/decoration/paint.rs:463-492`, `crates/compositor/src/switcher.rs:169-195`
  - **Cause** : `Painter::fitted()` retire un caractère à la fois et relance un shaping HarfBuzz+BiDi complet à chaque itération, puis `switcher()` re-shape 2 fois de plus. `selected` étant dans `SwitcherSpec`, chaque pression sur `Tab` re-rastérise toutes les tuiles en CPU et recrée un `MemoryRenderBuffer` (nouvelle texture GL).
  - **Impact** : 1 000 à 2 500 shapings HarfBuzz synchrones sur le thread principal à chaque `Tab` avec 15–25 fenêtres → lags de plusieurs dizaines de ms.
  - **Piste** : Recherche binaire (ou coupe directe depuis les `LayoutGlyph` en O(1) shaping) ; réutilisation du `Buffer` ; anneau de sélection rendu par shader GPU au lieu d'un repaint CPU.

- [ ] **#7 — `[Avéré]` Bug de cache négatif `freedesktop-icons 0.4` + `gsettings` synchrone sur le thread principal + `find_icon` dans `view()` du dock**
  - **Où** : `crates/apps/src/icons.rs:7-43`, `crates/compositor/src/decoration/icons.rs:24-86`, `crates/dock/src/ui/bar.rs:999`, `crates/dock/src/ui/toasts.rs:162`
  - **Cause** : `freedesktop-icons 0.4.0` ignore `CacheEntry::NotFound` dans son cache → chaque lookup d'une icône absente relit `index.theme` et exécute des centaines/milliers de `stat()`, doublés par le `.or_else(|| lookup("hicolor"))` redondant de `crates/apps/src/icons.rs:42`. Le dock appelle `find_icon` synchronement dans `view()` (`bar.rs:999`, `toasts.rs:162`) à chaque frame. Le compositeur exécute `Command::new("gsettings").output()` synchronement dans `IconCache::follow` (si Reversal absent) et décode SVG/PNG synchronement dans `render_screen()` (jusqu'à 3× par icône selon la taille 14/18/48 px).
  - **Impact** : Stutters de 5–50+ ms sur le compositeur et I/O disque synchrones à chaque frame sur le thread UI du dock.
  - **Piste** : Encapsuler `find_icon` dans un cache mémoire mémorisant aussi `None`, retirer le double fallback `hicolor`, sortir `gsettings` et la résolution/rastérisation d'icônes des threads UI/rendu.

- [ ] **#8 — `[Avéré]` Dock : tick horloge 1 Hz permanent redessinant TOUTES les surfaces wgpu chaque seconde**
  - **Où** : `crates/dock/src/app.rs:3699-3704`, `crates/dock/src/app.rs:2357-2364`
  - **Cause** : `iced::time::every(Duration::from_secs(1))` émet `Message::Tick` chaque seconde alors que l'horloge affiche `HH:MM` sans secondes. Dans `iced_layershell`, chaque message déclenche `request_refresh_all` → `view()` + rendu wgpu + `wl_surface.commit()` sur toutes les surfaces (en mode spatial 3 écrans : 6 surfaces wgpu/sec), réveillant aussi le compositeur et le flou Dual Kawase chaque seconde.
  - **Impact** : Wakeups CPU/GPU permanents à 1 Hz au repos et pendant le jeu ; perte d'autonomie sur portable 4K.
  - **Piste** : Aligner le timer sur le changement de minute (`60 - now.second()` secondes, 1 wakeup/min hors mode mock mixeur).

- [ ] **#9 — `[Avéré]` Dock : empreinte VRAM/RAM multi-surfaces (`iced_wgpu` duplique atlas GPU, worker et `fontdb` par surface)**
  - **Où** : `crates/dock/src/app.rs:1020-1196`, `crates/dock/src/app.rs:1247-1258`, `crates/dock/src/ui/spatial/bar.rs:22`, `vendor/iced_wgpu/src/image/atlas.rs:13`
  - **Cause** : Chaque surface `iced_layershell` instancie son propre `Renderer` avec un `image::Cache` non partagé : atlas `2048×2048` (16 Mo VRAM, 32 Mo en GL), thread `Worker`, `StagingBelt` (6 Mo), copie complète de `usvg::fontdb::Database` et swapchain propre. En mode spatial 3 écrans, 6 surfaces permanentes (dont 3 `coquille-home` plein écran même recouverts par des fenêtres) restent allouées, plus un `coquille-controls` plein écran pour un popover 348×420 px et des barres spatiales ~296 px de large (~81 % transparentes).
  - **Impact** : ~200–350+ Mo de VRAM permanents, 6+ threads worker et 6 copies de la base de polices en RAM.
  - **Piste** : Partager `image::Cache` dans `Engine` (`vendor/iced_wgpu`) ; ne garder `coquille-home` ouvert que sur les écrans sans fenêtre visible ; dimensionner le popover spatial à sa taille réelle.

---

## Élevés

- [ ] **#10 — `[Avéré]` Dual Kawase blur : passe 1:1 pleine résolution, pyramides `down[]` non mutualisées et re-captures via `blur_margin`**
  - **Où** : `crates/compositor/src/render/glass.rs:114-147`, `crates/compositor/src/render/glass.rs:590-668`, `crates/compositor/src/render/scene.rs:1265-1277`
  - **Cause** : Copie `glBlitFramebuffer` 1:1 et passe finale `up[0]` (8 taps) en pleine résolution 1:1 (jusqu'à 3840×2160 = 33,2 Mo, 8 à 12 passes en 4K). Chaque `GlassElement` garde ses propres textures intermédiaires `down[]` en VRAM (50–80 textures sur 3 écrans). `blur_margin` (32–76 px, jusqu'à 437 px à `strength = 5` en 4K) déborde du dock dans les fenêtres adjacentes à 8 px : un défilement/vidéo près du dock refloute toute la barre à 60–144 FPS. Idem pendant le fondu des drop zones (`overlay_alpha`).
  - **Impact** : Coût GPU/VRAM et consommation batterie élevés en 4K Intel ; saccades pendant `Super + glisser` ou scroll près du dock.
  - **Piste** : Blitter en demi-résolution (`GL_LINEAR`) et arrêter l'upsample à demi-résolution ; mutualiser un pool scratch `down[]` dans `SharedGl` ; passer l'opacité des drop zones en uniform au lieu de `Element::alpha`.

- [ ] **#11 — `[Avéré]` Overdraw 2,5×–3,2× dans `draw_glass` et coût excessif du shader `glass.frag` (`liquid_glass`)**
  - **Où** : `crates/compositor/src/render/glass.rs:891-901`, `crates/compositor/src/render/shaders/glass.frag:108-196`
  - **Cause** : `draw_glass` rastérise `dst = self.geometry` (`shape + 2 * blur_margin`, 60–70 % de pixels hors forme), et `glass.frag` ne fait aucun `discard` anticipé : sur la marge extérieure et le centre plat d'une pilule, il exécute 6 SDF, `pow`/`atan`/`sin`/`asin`/`tan`, une inversion `mat2` par pixel sur des uniforms constants, et 3 fetches de texture.
  - **Impact** : Gaspillage massif de cycles ALU/TMU, particulièrement sensible sur Intel 4K.
  - **Piste** : Restreindre `dst` et `damage` à `self.shape.intersection(dst)` dans `draw_glass` ; calculer la SDF en premier avec `if (d > 0.5) discard;` ; court-circuiter le centre plat (`-d >= bezel`) à 1 fetch ; précalculer la matrice inverse côté CPU.

- [ ] **#12 — `[Avéré]` Requêtes synchrones OpenGL (`glGetError`, `glGetIntegerv`) et `glTexParameteri` répétés dans `glass.rs`**
  - **Où** : `crates/compositor/src/render/glass.rs:496-586`, `crates/compositor/src/render/glass.rs:627-634`, `crates/compositor/src/render/glass.rs:880-887`
  - **Cause** : 2× `glGetError()`, jusqu'à 11 `glGetIntegerv`/`glIsEnabled` (dont `DRAW_FRAMEBUFFER_BINDING` et `VIEWPORT` lus deux fois) par `capture()`/`draw_mask()`, et `glTexParameteri` ×4 sur chaque passe (`2 * passes` fois) et sur `mask_tex` à chaque `draw_glass`.
  - **Impact** : Casse `mesa_glthread` sur Intel et sérialise le pipeline GL à chaque capture de flou.
  - **Piste** : Régler `glTexParameteri` une seule fois dans `ensure()` ; limiter `glGetError` au mode debug ou à la détection d'erreur ; supprimer les lectures d'état doublées.

- [ ] **#13 — `[Avéré]` Bug de coordonnées dans `GlassElement::damage_since`**
  - **Où** : `crates/compositor/src/render/glass.rs:341-351`
  - **Cause** : Retourne `self.geometry` en coordonnées sortie alors que Smithay attend des coordonnées locales à l'élément et y ajoute `element_loc` (double décalage à `(2*x, 2*y)`).
  - **Impact** : Dégâts parasites hors zone (re-flous inutiles ailleurs) ou défauts de rafraîchissement.
  - **Piste** : Retourner `Rectangle::from_size(self.geometry.size)`.

- [ ] **#14 — `[Avéré]` Désactivation permanente du direct scanout et du VRR dès le premier échec transitoire**
  - **Où** : `crates/compositor/src/backend/udev.rs:758-843`, `crates/compositor/src/game.rs:208-216`
  - **Cause** : Un seul refus transitoire (`EBUSY`) de `queue_frame` appelle `scanout_failed()` et `vrr_failed()`, coupant les deux définitivement pour le reste de la session et tous les jeux suivants.
  - **Impact** : Perte silencieuse et irréversible du VRR / direct scanout jusqu'au redémarrage.
  - **Piste** : Compteur d'échecs consécutifs (ex. 3–5 frames) et réinitialisation à chaque nouveau jeu ancré.

- [ ] **#15 — `[Avéré]` `CompositorHandler::commit` : travail redondant à 240 FPS et redraws de surfaces invisibles / panneaux**
  - **Où** : `crates/compositor/src/handlers/compositor.rs:72-117`, `crates/compositor/src/panels.rs:193-264`
  - **Cause** : Chaque `wl_surface.commit` exécute 2 scans O(W) des fenêtres, `map_window`, 2 remontées `get_parent`, 3 lookups `layer_map_for_output` dans `commit_panel`, les popups, puis `queue_redraw()` inconditionnellement — même pour un onglet caché, une fenêtre réduite ou masquée derrière un jeu. Sur commit d'un panneau animé, `commit_panel()` exécute `map.arrange()`, `tell_panels_scale()` et 2× `self.panels()` à chaque frame.
  - **Impact** : Surcoût CPU par frame en jeu 240 FPS ; réveils GPU continus causés par des fenêtres invisibles.
  - **Piste** : Brancher exclusivement `window` vs `panel` vs `popup`, ignorer `queue_redraw()` si `!managed.geometry.visible` (ou subsurface sync), et n'appeler `arrange_panels()` que si l'état layer-shell change.

- [ ] **#16 — `[Avéré]` Re-rastérisation CPU + nouvelle texture GL sur resize/split/hover/titre + `capsule_group` 5–10× trop large**
  - **Où** : `crates/compositor/src/shell.rs:467-553`, `crates/compositor/src/decoration/paint.rs:268-364`
  - **Cause** : Chaque changement de `TitlebarSpec` / `GroupSpec` (resize, split, titre de terminal, hover d'un bouton) repeint toute la barre en CPU et recrée un `MemoryRenderBuffer::from_slice` (copie + nouvel `Id` Smithay + `glTexImage2D`, ~1,16 Mo en 4K). `capsule_group` alloue sa texture sur toute la largeur de la bande (jusqu'à 3840×64 px ≈ 983 Ko, 80–95 % transparents) au lieu de `layout.rect` et shape chaque label deux fois. `rename_window` ne court-circuite pas quand `app_id` et `title` sont inchangés.
  - **Impact** : Charge CPU/VRAM et damage inutiles sur resize/split/hover/titres animés.
  - **Piste** : Allouer `capsule_group` à `layout.rect.size()` ; réutiliser `MemoryRenderBuffer::render()` avec damage partiel ; court-circuiter `rename_window` sur valeur identique.

- [ ] **#17 — `[Avéré]` I/O bloquantes sur le thread principal du compositeur : `Applications::identity`, `wallpaper`, `screenshot`**
  - **Où** : `crates/compositor/src/decoration/naming.rs:30-41`, `crates/apps/src/catalog.rs:192-261`, `crates/compositor/src/wallpaper.rs:295-412`, `crates/compositor/src/screenshot.rs:57-242`, `crates/compositor/src/render/scene.rs:1067-1081`
  - **Cause** :
    - Pour tout `app_id` inconnu (Proton `steam_app_*`, binaires dev), `Applications::identity` exécute synchronement `fingerprint()` (`stat` de tous les `.desktop`) et éventuellement `Catalog::load()` ; `Catalog::find` n'est pas indexé (4 scans O(N) avec `.to_lowercase()`, appelés 2×).
    - `wallpaper.rs` lit, décode et redimensionne en bicubique CPU (même à taille 1:1) synchronement, duplique entre écrans identiques (`DP-1`/`HDMI-A-1`) et garde RAM + VRAM (~36 Mo ×2) via `MemoryRenderBuffer`.
    - `screenshot.rs` encode le PNG (`save_png`) synchronement sur le thread principal (~100–400 ms de gel) et dessine l'overlay freeze sans `ShapeCache` (redessin 100 % des 3 écrans à chaque mouvement souris).
  - **Impact** : Freezes complets du compositeur (50–500 ms) au changement de thème, à la capture d'écran ou à l'ouverture d'applis inconnues.
  - **Piste** : Déporter `Catalog` reload, `wallpaper` decode/scale et `screenshot` PNG sur des worker threads ; indexer `Catalog` en `HashMap` O(1) ; court-circuiter `scale_pixmap` en 1:1 et utiliser `TextureRenderElement::from_static_texture` ; utiliser `overlay_shapes` en mode freeze.

- [ ] **#18 — `[Avéré]` Dock : `image::Handle::from_rgba` appelé dans `view()` → `Id::unique()` → cache miss GPU à chaque frame**
  - **Où** : `crates/dock/src/ui/bar.rs:962-971`, `crates/dock/src/ui/toasts.rs:138-147`
  - **Cause** : `image::Handle::from_rgba` attribue un `Id::unique()` incrémental à chaque appel dans `view()`. Chaque icône tray pixmap (SNI/XEmbed Wine/Proton) et chaque notification `Icon::Pixels` (jusqu'à 1 Mo) est clonée sur le thread UI et ré-uploadée dans l'atlas GPU à chaque frame.
  - **Impact** : Thrashing continu de l'atlas VRAM et copies mémoire répétées.
  - **Piste** : Pré-construire et stocker le `image::Handle` une seule fois dans `TrayPixmap` / `Notice`, et simplement `.clone()` le `Handle` dans `view()`.

- [ ] **#19 — `[Avéré]` Dock : spawning continu de sous-processus (`nmcli`, `bluetoothctl`, `upower`, `pactl`) + scan disque 3 s (`installed.rs`)**
  - **Où** : `crates/dock/src/backend/services.rs:117-205`, `crates/dock/src/backend/battery.rs:54-88`, `crates/dock/src/backend/audio.rs:145-292`, `crates/dock/src/backend/installed.rs:17-48`
  - **Cause** : 4 `fork+exec` (`nmcli` ×2 + `bluetoothctl` ×2) toutes les 5 s = **2 880 processus/heure** même centre de contrôle fermé ; `upower -i` toutes les 60 s et sans drain sur rafales de `upower --monitor` ; 3 `pactl` séquentiels sans debounce dans `audio.rs` (+ double parsing JSON de `sinks_json`) ; scan `fingerprint()` complet des `.desktop` et `steamapps/` toutes les 3 s dans `installed.rs`.
  - **Impact** : Consommation CPU idle, dégradation des C-states et autonomie batterie sur portable, latence sur le slider audio.
  - **Piste** : Remplacer `nmcli`/`bluetoothctl`/`upower` par des abonnements D-Bus `PropertiesChanged` via `zbus` (déjà dépendance) ; remplacer le poll 3 s par `inotify` ; débouncer `audio.rs` (50–100 ms).

- [ ] **#20 — `[Avéré]` Dock : `ExpireToasts` à 4 Hz (250 ms) — boucle infinie sur toast sticky (alerte batterie 5 %)**
  - **Où** : `crates/dock/src/app.rs:3706-3709`, `crates/dock/src/app.rs:1780`, `crates/dock/src/backend/notify.rs:222`
  - **Cause** : Tant que `notify.visible` est non vide, un timer 4 Hz émet `ExpireToasts`, forçant `request_refresh_all` sur toutes les surfaces. Un toast sticky (`expire_ms: None`, ex. alerte batterie critique 5 %) fait tourner tout le dock à 4 FPS indéfiniment.
  - **Impact** : Charge GPU/CPU soutenue précisément lorsque la batterie est critique.
  - **Piste** : Armer un unique timer one-shot jusqu'à la prochaine échéance `expire_ms.is_some()` (aucun timer si tous les toasts visibles sont sticky).

- [ ] **#21 — `[Avéré]` Dock : recalculs massifs dans `view()` (`identify()`, `filter_apps()`, `started_games()`, `plain_text()`) + polling HTTPS 60 s hors visibilité**
  - **Où** : `crates/dock/src/ui/spatial/launcher.rs:241-386`, `crates/dock/src/ui/spatial/cards.rs:507-509`, `crates/dock/src/ui/controls.rs:421-707`, `crates/dock/src/model/mod.rs:57-123`, `crates/dock/src/app.rs:1580-1678`, `crates/dock/src/backend/tasks.rs:56-67`, `crates/dock/src/backend/foundry.rs:101-116`
  - **Cause** : `dock.identify()` scanne les jeux Steam (avec `format!("steam_app_{}")` par jeu) puis `Catalog::find` (4× O(N) + `.to_lowercase()`), appelé jusqu'à 80–90 fois par `view()` en mode spatial (et à 12,5 Hz dans le mixeur). `filter_apps`, `started_games` (24 items × 3 écrans) et `plain_text` (×200) recalculent à chaque `view()`. Côté réseau, les widgets spatiaux font 3 requêtes HTTPS/min même tiroir fermé ou jeu ancré, en recréant un `ureq::Agent` neuf à chaque minute (pas de keep-alive / cache TLS).
  - **Impact** : Latence de frappe, churn allocateur CPU sur `view()`, réveils réseau/Wi-Fi et charge TLS inutiles.
  - **Piste** : Indexer `Catalog` et jeux Steam en `HashMap` ; pré-calculer `Identity`, `fold` et `plain_text` à l'ingestion ; partager un `ureq::Agent` (`LazyLock`) et ne poller que lorsqu'un lanceur spatial est visible hors jeu ancré.

---

## Moyens

- [ ] **#22 — `[Avéré]` `coquille-settings` : 2× `pactl` bloquants sur le thread UI à chaque `Event::State` du compositeur**
  - **Où** : `crates/settings/src/app.rs:158-193`, `crates/settings/src/app.rs:322-327`, `crates/settings/src/model.rs:567-583`
  - **Cause** : Chaque changement de focus ou de titre de fenêtre déclenche `read_file()` → relecture `config.toml` + 2 `Command::new("pactl").output()` synchrones sur le thread UI, même hors page Audio.
  - **Piste** : Ne relire la config que sur `Event::ConfigChanged` et charger les sinks en `spawn_blocking` uniquement à l'ouverture de la page Audio.

- [ ] **#23 — `[Avéré]` `refresh_chrome()`, `refresh_switcher()`, `keep_animating()` et `panels()` exécutés N fois par cycle (par écran)**
  - **Où** : `crates/compositor/src/backend/udev.rs:665-692`, `crates/compositor/src/backend/udev.rs:880`, `crates/compositor/src/animate.rs:213-237`, `crates/compositor/src/shell.rs:417-537`
  - **Cause** : Exécutés 3 fois par cycle sur 3 écrans avec allocations de `TitlebarSpec`/`GroupSpec` (`String`/`Vec`) juste pour comparer `==`. `keep_animating()` réveille les 3 écrans tant qu'une animation joue sur 1 écran et peut purger une animation avant que les écrans suivants ne dessinent leur dernière frame.
  - **Piste** : Appeler `refresh_chrome`/`refresh_switcher` une fois avant la boucle (avec dirty flag + comparaison par `&str`), et `keep_animating()` une fois après la boucle en marquant uniquement les `OutputId` animés.

- [ ] **#24 — `[Avéré]` `pieces()` alloue à chaque frame immobile et conserve en VRAM les textures des fenêtres cachées/réduites**
  - **Où** : `crates/compositor/src/render/scene.rs:162-189`, `crates/compositor/src/render/scene.rs:426-428`, `crates/compositor/src/render/scene.rs:1141`
  - **Cause** : Recrée `Vec<Piece>` + `Id::new()` à chaque frame immobile, et `chrome.snapshot` garde un `Arc<GlesTextureInternal>` (`EGLImage`) qui n'est jamais vidé lorsque la fenêtre devient cachée ou réduite.
  - **Piste** : Ne rafraîchir `snapshot` que si le `CommitCounter` change, et exécuter `chrome.snapshot.clear()` dès que `!geometry.visible`.

- [ ] **#25 — `[Avéré]` IPC : déconnexion brutale du dock sur `WouldBlock` + construction `Snapshot` O(W²) avec double clone**
  - **Où** : `crates/compositor/src/ipc.rs:42-55`, `crates/compositor/src/ipc.rs:201-274`, `crates/compositor/src/shell.rs:854-869`
  - **Cause** : `broadcast()` utilise `write_all()` sur socket non-bloquant et expulse immédiatement le client sur `WouldBlock`. `snapshot()` est en O(W²), clone tout le `Snapshot` une 2e fois pour sérialiser, et tourne inutilement pendant les drags ou les `rename_window` identiques.
  - **Piste** : Tampon sortant borné par client avec `Interest::WRITE` ; sérialiser par référence (`&Snapshot`) ; ne marquer `ipc.stale` que lorsque les champs exposés en IPC changent.

- [ ] **#26 — `[Avéré]` Complexité O(W²) / O(W² log W) dans `Desk::arrangement()`, `Shell::sync()` et `State::manage()`**
  - **Où** : `crates/model/src/desk.rs:1211-1295`, `crates/compositor/src/shell.rs:398-412`, `crates/compositor/src/shell.rs:677-714`
  - **Cause** : `Desk::geometry()` rappelle `self.focused()` pour chaque fenêtre ; `Shell::sync()` fait un lookup linéaire dans `sort_by_key` ; `State::manage()` calcule `arrangement()` jusqu'à 3 fois à l'ouverture d'une fenêtre.
  - **Piste** : Calculer `focused` une seule fois en tête d'`arrangement()` et indexer les positions en O(W) dans `Shell::sync()`.

- [ ] **#27 — `[Avéré]` Écritures disque, roundtrip X11 et chargement curseurs/VDF/tray synchrones**
  - **Où** : `crates/compositor/src/persist.rs:58-64`, `crates/compositor/src/xwayland.rs:229-251`, `crates/compositor/src/cursor.rs:52-93`, `crates/dock/src/app.rs:3037-3050`, `crates/dock/src/app.rs:3888-3898`, `crates/dock/src/steam.rs:200-352`, `crates/dock/src/backend/tray.rs:293-395`
  - **Cause** :
    - `memory.json` et `proportions.json` écrits synchronement dans `apply_arrangement()` ; `volumes.json` écrit à chaque pas de slider audio ; `dock::spawn()` utilise `Command::status()` bloquant.
    - `update_xwayland_primary` fait un roundtrip X11 synchrone (`get_randr_primary_output`) depuis le thread principal du compositeur.
    - `CursorImages::get` charge et décode toutes les tailles/frames XCursor synchronement dans `render_screen()` sans tester `alt_names()`.
    - `steam::load_playtimes` tokenise tout `localconfig.vdf` en centaines de milliers de `String` sur le thread principal au boot du dock.
    - `backend/tray.rs` exécute `std::fs::read_dir` récursif sur le runtime async tokio à chaque événement SNI et recrée une `zbus::Connection::session()` à chaque clic droit.
  - **Piste** : Déporter les écritures/lectures dans des workers / `spawn_blocking`, débouncer `volumes.json`, précharger les curseurs courants, parser VDF en streaming `&str`, cacher les chemins d'icônes tray et réutiliser la connexion D-Bus.

- [ ] **#28 — `[Avéré]` Zombies `Child`, fuite de tokens `xdg-activation-v1`, RAM notifications (200 MiB), `fit_bars()` et `Glyph::handle()`**
  - **Où** : `crates/compositor/src/state.rs:562-586`, `crates/compositor/src/handlers/xdg_activation.rs:17-37`, `crates/dock/src/model/notify.rs:18-20`, `crates/dock/src/app.rs:1277-1348`, `crates/ui/src/glyph.rs:160-176`
  - **Cause** :
    - `State::spawn` et `wl-copy` abandonnent leurs `Child` sans reaping `SIGCHLD` → processus zombies (`Z`).
    - `xdg_activation_state.retain_tokens(...)` n'est jamais appelé → fuite lente sur sessions longues.
    - Historique notifications (200 entrées) stocke `Icon::Pixels` jusqu'à 512×512 (1 MiB) pour un rendu 20–24 px → jusqu'à ~200 MiB RAM.
    - `fit_bars()` ré-émet `ExclusiveZoneChange` et `SetInputRegion` (avec `wl_surface.commit()`) sans vérifier si les valeurs ont changé.
    - `Glyph::handle()` reformate le XML SVG, alloue un `Vec<u8>` et le hache à chaque `view()`.
  - **Piste** : Handler `SIGCHLD` dans calloop ; appeler `retain_tokens` ; redimensionner `Icon::Pixels` à 64×64 à l'ingestion ; dédupliquer `fit_bars()` ; cacher `Glyph::handle()` dans un `LazyLock<[svg::Handle; N]>`.

- [ ] **#29 — `[Potentiel]` `RenderFrameResult::needs_sync()` non vérifié avant `queue_frame()`**
  - **Où** : `crates/compositor/src/backend/udev.rs:781-825`
  - **Cause** : `draw()` ignore `frame.needs_sync()`. Sain quand le pilote exporte les fences EGL natives, mais risque de scanout avant fin de rendu GPU sur tout chemin sans fence exportable.
  - **Piste** : Synchroniser avant `queue_frame()` lorsque `frame.needs_sync()` est `true`.

---

## Faibles

- [ ] **#30 — `[Avéré]` Rendu de texte `Painter::text` : shaping/parcours hors zone clippée et blitting pixel par pixel**
  - **Où** : `crates/compositor/src/decoration/paint.rs:537-626`
  - **Piste** : Itérer `buffer.layout_runs()` avec arrêt dès `glyph.x * s >= clip_right` et blitter les `SwashImage` par spans via `SwashCache::get_image`.

- [ ] **#31 — `[Potentiel]` Croissance non bornée de `SwashCache` et `IconCache` sur sessions longues**
  - **Où** : `crates/compositor/src/decoration/paint.rs:141`, `crates/compositor/src/decoration/icons.rs:18`, `crates/compositor/src/decoration/icons.rs:45`
  - **Piste** : Borner `SwashCache.image_cache` et `IconCache.drawn` ; éviter l'allocation `name.to_owned()` sur cache hit dans `IconCache::get`.

- [ ] **#32 — `[Avéré]` `coquille-polkit` abonné en permanence à tous les `Event::State` + feature `mundy` inutile sur `iced_layershell`**
  - **Où** : `crates/polkit/src/app.rs:93-97`, `crates/dock/Cargo.toml:26`, `crates/polkit/Cargo.toml:16`
  - **Piste** : Désactiver `default-features` (`linux-theme-detection` / `mundy`) sur `iced_layershell` ; côté `polkit`, lire `theme`/`accent` à l'ouverture du dialogue plutôt que désérialiser chaque `Event::State`.

- [ ] **#33 — `[Avéré]` Pollings mineurs au repos (`backlight` 2 s, `keyring` `/proc/<pid>` 1 Hz + `fsync` sous `Mutex`, `winit` 60 Hz)**
  - **Où** : `crates/dock/src/backend/services.rs:274-301`, `crates/keyring/src/main.rs:116-121`, `crates/keyring/src/vault.rs:104-120`, `crates/compositor/src/main.rs:465-471`
  - **Piste** : Uevents ou lecture à la demande pour le rétroéclairage ; `pidfd_open` pour `keyring --until` ; n'armer le timer `winit` que si `redraw_needed`.

- [ ] **#34 — `[Avéré]` Petits nettoyages & allocations mineures**
  - **Où** : `crates/compositor/src/render/scene.rs:916-919` (`PanelMotion::entry` non vidé après fin d'animation), `crates/compositor/src/backend/winit.rs:359` (`panels()` dans `send_frame_callbacks`), `crates/compositor/src/handlers/layer_shell.rs:58-79` & `crates/compositor/src/backend/udev.rs:921-926` (`State::liquid` et `suppressed_keys` non purgés), `crates/compositor/src/input.rs:1695-1696` (`Vec` dans `TitlebarLayout::new`), `crates/ipc/src/lib.rs:235-251` (`LineBuffer::push` `drain` par ligne).
  - **Piste** : Purger ces états à la fin d'animation / destruction / `PauseSession` et remplacer les petites allocations par des structures fixes/réutilisées.

---

## Points déjà sains (vérifiés)

- Pas de timer périodique au repos sur `udev` ; `keep_animating()` et `Dock::animating()` s'arrêtent proprement dès la fin des animations.
- `set_glass` IPC dédupliqué (`glass_told`) et sockets IPC non-bloquants côté compositeur.
- `FontSystem`/`SwashCache` uniques, `SharedGl` + arrondi `ALLOCATION_STEP = 64 px` dans `glass.rs`, `ShapeCache`/`ShadowCache`/`PerView`, `image::Handle::from_path` bien caché en VRAM.
- `AppTheme` et `xrdb` déjà sur threads dédiés ; `parec` lancé uniquement mixeur ouvert et tué à la fermeture ; `coquille-polkit` détruit son compositor wgpu au repos (0 VRAM).
- Synchronisation explicite (`linux-drm-syncobj`) et isolation du jeu ancré côté `scene.rs` (sans `ClippedSurface`, sans flou, sans panneaux).
