# 01 — Jetons du verre : une crate de jetons d'apparence

- **Priorité :** première vague (sans dépendance)
- **Nature :** dette et apparence
- **Zone :** `crates/theme` (nouvelle), `crates/ui/src/style.rs`, `crates/compositor/src/theme.rs`, `crates/compositor/src/decoration/naming.rs`, `crates/dock/src/ui/icons.rs`, `crates/settings/src/model.rs`
- **Se vérifie :** tests unitaires, puis captures en imbriqué
- **Certitude :** lu dans le code

Lire d'abord [le README de la refonte](README.md) : son but, la **règle
cardinale** (aucune disposition ne change) et le tableau des jetons.

## But

Une seule source pour les couleurs du shell, puis la nouvelle palette de verre
posée dedans. Ce ticket **réalise le ticket [21](../21-crate-de-jetons.md)**
(crate de jetons sans iced) et règle au passage le
[17](../17-couleur-de-repli.md) (couleur de repli d'une appli). Les deux sont
à relire avant de commencer.

## Constat

Les mêmes valeurs sont écrites deux fois, sans lien dans le code :

- `crates/compositor/src/theme.rs:80-109`, `Theme::new(ThemeKind)` : le
  compositeur a sa table `backdrop, surface, titlebar, separator, text,
  text_muted, disabled, accent, button_hover, close_hover`, en `Rgba` (octets,
  sRGB non prémultiplié).
- `crates/ui/src/style.rs:56-87`, `Palette::new(ThemeName, Option<Color>)` : le
  dock et l'écran « Paramètres » ont `bar, bar_border, panel, block, card,
  hover, separator, text, muted, scrim`, en `iced::Color`.
- L'accent par défaut (`#2f6fed` en clair, `#7aa7ff` en sombre) est écrit des
  deux côtés et figé par le test `accents_match_the_design`
  (`crates/ui/src/style.rs:195`).
- La couleur de repli d'une appli diffère : `badge_color`
  (`decoration/naming.rs:39`) contre `tile_color` (`dock/src/ui/icons.rs:99`).

## Piste

1. **Nouvelle crate `crates/theme`** (paquet `coquille-theme`), sans aucune
   dépendance graphique :
   - un type de couleur en octets RGBA, avec la lecture et l'écriture de
     `#rrggbb` ;
   - pour chaque thème (`Light`, `Dark`), **tous les jetons du tableau du
     README** : `backdrop`, `glass`, `glass_strong`, `rim`, `card`, `hover`,
     `separator`, `text`, `muted`, `shadow`, `scrim`, `accent` par défaut ;
   - les jetons propres au compositeur qui ne s'y ramènent pas (`disabled`,
     `close_hover`…), dans la même table ;
   - la couleur de repli d'une appli. On garde la palette et le hachage du
     compositeur (`badge_color`), et un test vérifie que le dock et le
     compositeur donnent la même couleur pour une même clé.
2. **Le compositeur** (`theme.rs`) construit son `Theme` depuis la crate et
   n'écrit plus aucune valeur. Les champs gardent leur nom tant qu'ils servent
   ; on ajoute ceux dont les tickets 02 à 04 auront besoin (`glass`,
   `glass_strong`, `rim`, `card`, `shadow`).
3. **`ui`** (`style.rs`) construit sa `Palette` depuis la crate. Les champs
   existants **restent**, pour ne rien casser dans le dock et les Paramètres ;
   ils sont alimentés par les nouveaux jetons :
   - `bar` ← `glass` ;
   - `bar_border` ← `rim` ;
   - `panel` ← `glass` ;
   - `block` et `card` ← `card` ;
   - `hover`, `separator`, `text`, `muted` et `scrim` ← leurs homonymes.

   On ajoute `glass_strong`, `rim` et `shadow` pour les tickets 06 et 07.
   `Palette::shadow()` (l.91) prend la couleur `shadow` ; sa forme (décalage,
   rayon de flou) ne change pas.
4. **Accent pervenche** : `#5b63e6` en clair, `#8f97ff` en sombre. On met à
   jour le test `accents_match_the_design` et le texte de
   `config/config.toml:14-15`, qui cite les anciens accents. Les autres
   mentions de `#2f6fed` et `#7aa7ff` sont des couleurs d'essai pour la
   lecture du réglage (`compositor/src/config.rs`, `theme.rs`,
   `dock/src/config.rs`) ; elles peuvent rester. On
   ajoute `#5b63e6` en tête de `ACCENTS` (`crates/settings/src/model.rs:108`),
   le tableau des pastilles d'accent de l'écran « Paramètres ». C'est une
   pastille de plus dans la rangée existante : vérifier en capture qu'elle
   tient sans déborder.
5. `Cargo.toml` du workspace : on ajoute le membre. Le compositeur, `ui` et
   le dock en dépendent.

Ce ticket **ne flotte rien** : sans le ticket 02, les surfaces translucides
laissent simplement voir le fond d'écran net. C'est attendu.

Règle des alphas (README) : scrim et ombres sous 0.18 d'alpha, verre au-dessus
de 0.45. Un test de la crate le vérifie pour chaque thème.

## Terminé quand

- Plus aucune valeur de couleur n'est écrite hors de `crates/theme`, sauf
  `DANGER` et `LEVEL` de `ui/src/style.rs` et les couleurs des icônes.
- Tests : accents par défaut, règle des alphas, couleur de repli identique des
  deux côtés, lecture de `#rrggbb`.
- Captures en imbriqué, avant et après, en clair et en sombre : compositeur
  (deux fenêtres rangées et une flottante), dock, tiroir et « Paramètres ».
  Les dispositions se superposent ; seules les couleurs changent.
- `CLAUDE.md`, arbre des crates : la nouvelle crate y figure.
- Les tickets 21 et 17 sont supprimés dans le même commit, et
  `docs/tickets/README.md` est mis à jour.

## Liens

- Réalise : [21](../21-crate-de-jetons.md), [17](../17-couleur-de-repli.md).
- Débloque : 03, 04, 06, 07.
