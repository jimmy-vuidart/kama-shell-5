# 04 — Décorations du compositeur en verre

- **Priorité :** deuxième vague, après **01** et **02**, et après **05** (même fichier `render/scene.rs`)
- **Nature :** apparence
- **Zone :** `crates/compositor/src/decoration/paint.rs`, `render/scene.rs`, `input.rs`, `backend/udev.rs`, `backend/winit.rs`, `screenshot.rs`
- **Se vérifie :** captures en imbriqué
- **Certitude :** lu dans le code

Lire d'abord [le README de la refonte](README.md) : la **règle cardinale**
(aucune disposition ne change) et le tableau des jetons.

## But

Les barres de titre, les capsules, la barre de rangement et les zones de
dépôt deviennent du verre : translucides, floutées, avec un liseré. **Formes,
tailles, contenus, textes, icônes et boutons restent exactement ceux
d'aujourd'hui.**

## Constat

Ces décorations sont peintes sur le processeur (tiny-skia et cosmic-text),
puis envoyées comme textures (`MemoryRenderBuffer`, `Fourcc::Abgr8888`). Le
peintre prend ses couleurs dans `Theme::new(spec.theme).with_accent(spec.accent)`
(`paint.rs:141`, `:234`, `:357`). Toutes sont opaques, sauf le fond du groupe de
capsules (`BOX_OPACITY = 217`, `paint.rs:233`).

| Décoration | Peinte par | Remplissage aujourd'hui |
|---|---|---|
| Barre de titre | `Painter::titlebar` (`paint.rs:140`) | `titlebar`, coins du haut arrondis, filet `separator` en bas, pilule de survol par bouton |
| Groupe de capsules | `Painter::capsule_group` (`paint.rs:231`) et `capsule` (`paint.rs:270`) | boîte `titlebar` à 217/255, capsule affichée sur `button_hover`, anneau d'accent de 2 px pour la fenêtre active |
| Barre de rangement | `Painter::placement_bar` (`paint.rs:349`) | `titlebar` opaque, filet `separator`, premier bouton plein d'accent |
| Zones de dépôt | `input.rs:1043-1070` (`Shape`) | contour d'accent (alpha 0x8c, 0xff visé) et fond d'accent (0x1a, 0x4d visé) |

## Piste

1. **Peinture** (`paint.rs`), sans toucher aux mesures :
   - barre de titre : fond `glass` au lieu de `titlebar` ; filet du bas en
     `separator` ; liseré `rim` de 1 px le long du bord haut, dans les coins
     arrondis ; survol des boutons en `hover` ;
   - groupe de capsules : boîte en `glass` (`BOX_OPACITY` disparaît) et
     liseré `rim` ; capsule affichée en `glass_strong` ; l'anneau d'accent et
     la capsule grisée d'une fenêtre réduite ne changent pas ;
   - barre de rangement : fond `glass_strong`, liseré `rim`, premier bouton
     toujours plein d'accent ;
   - le texte reste en `text` / `muted` ; vérifier sa lisibilité sur le verre
     clair comme sur le sombre.
2. **Le verre derrière** (`scene::build`). Un `GlassElement` (ticket 02),
   `Mask::Rounded`, avec la forme et les rayons exacts de chaque décoration,
   est placé **juste après** elle dans la liste :
   - barre de titre : rayons du haut `corner_radius`, rayons du bas 0
     (`scene.rs:353-368`) ;
   - chaque groupe de capsules : la boîte du groupe, pas toute la bande
     (`scene.rs:521-540`) ; la géométrie de la boîte se lit dans
     `decoration/capsules.rs` ;
   - barre de rangement (`scene.rs:468-483`) ;
   - chaque zone de dépôt (`scene.rs:485-494`).

   Garder l'état de chaque `GlassElement` entre deux images (`Chrome` pour les
   fenêtres, l'équivalent pour les groupes et la barre), sans quoi chaque
   image recapture tout.
3. **Zones de dépôt** (`input.rs:1043-1070`) : verre flouté et teinté
   d'accent (fond d'accent à faible alpha par-dessus le flou), et liseré
   `rim` en plus du contour d'accent actuel. Les tailles et la logique
   « visé / pas visé » ne changent pas.
4. **Couleur de fond** : la couleur d'effacement devient `backdrop`
   (`backend/udev.rs:669`, `backend/winit.rs:247`, `screenshot.rs:129`). Le
   noir d'un écran ancré ne change pas. Le fond d'écran reste celui
   d'aujourd'hui.
5. **`[glass] blur = false`** (ticket 02) : aucun `GlassElement` n'est placé ;
   les décorations restent translucides, sur `glass_strong` pour rester
   lisibles.
6. **Mode jeu** : rien de tout cela n'existe sur un écran ancré (pas de
   décoration), et aucun `GlassElement` ne doit y être créé.

## Terminé quand

- Captures en imbriqué, en clair et en sombre, superposables à celles d'avant
  à la matière près :
  - deux fenêtres rangées : capsules et barres de titre en verre, fond d'écran
    flouté à travers ;
  - une fenêtre flottante avec ses boutons ;
  - la barre de rangement d'une fenêtre qui vient de s'ouvrir ;
  - un glisser avec `Mod` (`--debug-input` : `press alt`, `down`,
    `move…`) : zones de dépôt ;
  - une fenêtre réduite (capsule grisée).
- `[glass] blur = false` puis rechargement à chaud : plus de flou, tout reste
  lisible.
- Écran ancré : aucun `GlassElement` (vérifié par un `debug!` qui les compte
  par écran), affichage direct inchangé dans le journal.
- `scripts/ci.sh` est vert.

## Liens

- Dépend de : 01 (jetons), 02 (`GlassElement`).
- Partage `render/scene.rs` avec 03 et 05 : passer après 05, avant 03.
