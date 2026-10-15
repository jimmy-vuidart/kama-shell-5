# Refonte « verre liquide »

Ces tickets ont été écrits le 15 octobre 2026, sur le commit `9198c18`. Les
numéros de ligne cités sont ceux de ce commit. Chaque ticket peut être confié à
un agent différent, qui part sans autre contexte que ce dossier, `CLAUDE.md` et
le code.

## Le but

On quitte les aplats blancs ou anthracite presque opaques pour du **verre
liquide**. Les surfaces du shell deviennent translucides et **vraiment
floutées** : ce qui est derrière transparaît, dépoli. Elles ont un liseré
clair de 1 px, des ombres douces teintées et des nuances pastel. Le thème
sombre est une variante indigo translucide.

Les images de [`references/`](references/) donnent l'ambiance : matière,
transparence, couleurs, ombres. **Elles ne donnent pas la disposition.**

- [`dock.png`](references/dock.png) : barre de verre clair.
- [`centre-de-controle.png`](references/centre-de-controle.png) : cartes de verre sur fond pastel.
- [`planche-de-style.png`](references/planche-de-style.png) : couleurs, matières, ombres.
- [`parametres-sombre.png`](references/parametres-sombre.png) : variante sombre, indigo translucide.

## Règle cardinale : aucune disposition ne change

Restent ceux d'aujourd'hui :

- les tailles, positions, rayons et espacements ;
- les contenus et leur ordre ;
- `crates/model/src/metrics.rs` ;
- les widgets iced, aux mêmes places.

Seuls changent **les couleurs, l'opacité, le flou, les liserés et les
ombres**. Une capture avant et une capture après doivent se superposer trait
pour trait, à la matière près.

## Décisions de Jimmy

- **Vrai flou** de l'arrière-plan, calculé par le compositeur. Il est coupé sur
  un écran ancré (mode jeu) : aucun coût en jeu, l'affichage direct reste
  possible.
- **Le fond d'écran actuel est conservé** (`assets/wallpapers/glass-*.png`).
- **Périmètre** : compositeur (décorations), dock, écran « Paramètres ».
- Pas de thème « Auto ». La police reste **Figtree**.
- **Aucun nouveau réglage à l'écran** : la section `[glass]` se règle
  seulement dans `config.toml`.
- **Nouvel accent par défaut, pervenche.** Un accent choisi dans `config.toml`
  reste prioritaire.

## Comment le flou est possible

Le Smithay épinglé (commit `19c16d3`, le même que niri 26.04, qui a livré le
flou) fournit les **framebuffer effects**
(`~/.cargo/git/checkouts/smithay-*/19c16d3/src/backend/renderer/element/mod.rs:556-606`) :

- un élément dont `Element::is_framebuffer_effect()` rend `true` voit
  `RenderElement::capture_framebuffer(frame, src, dst, cache: &UserDataMap)`
  appelé **avant** son `draw`, et seulement si ce qu'il y a derrière lui a
  changé ;
- le suivi des dégâts redessine alors toute la zone derrière lui
  (`backend/renderer/damage/mod.rs`, vers la ligne 690) ;
- `GlesFrame` implémente `BlitFrame::blit_to` (`backend/renderer/gles/mod.rs:1785`) ;
- `frame.with_context` donne accès au GL brut.

Niri 26.04 (`src/render_helpers/framebuffer_effect.rs`, `blur.rs`, shaders
`blur_down.frag` et `blur_up.frag`) est une **référence de technique
seulement**. Il est sous GPL-3 : **on ne copie pas son code**. Le Kawase double
est un algorithme public.

Le dock (`iced_layershell`) et l'écran « Paramètres » ne savent pas demander
le flou par le protocole `ext-background-effect`. C'est donc le compositeur qui
floute derrière **nos** programmes. La forme du flou suit **le canal alpha de
leur surface** : rien à synchroniser, et menus, infobulles et barre qui se
déploie suivent d'eux-mêmes.

## Les jetons

Ce sont des valeurs de départ, à ajuster sur captures. La source unique est la
crate de jetons du ticket 01.

| Jeton | Rôle | Clair | Sombre |
|---|---|---|---|
| `backdrop` | couleur de fond, sous le fond d'écran | `#e9e6f4` | `#1c1b3a` |
| `glass` | barre, tiroir, barres de titre, barre latérale | blanc à 0.55 | `rgba(42,40,92,.50)` |
| `glass_strong` | menus, infobulles, mixeur, capsule affichée | blanc à 0.80 | `rgba(36,34,78,.82)` |
| `rim` | liseré de 1 px | blanc à 0.65 | blanc à 0.12 |
| `card` | tuile ou bloc posé sur du verre | blanc à 0.60 | blanc à 0.08 |
| `hover` | survol | blanc à 0.35 | blanc à 0.10 |
| `separator` | filets | `rgba(30,34,60,.08)` | blanc à 0.08 |
| `text` / `muted` | textes | `#1d2033` / `#6b6f85` | `#f2f1fb` / `#a7a6c8` |
| `shadow` | ombres douces | `rgba(60,50,120,.14)` | `rgba(8,6,30,.35)` |
| `scrim` | voile derrière le tiroir | `rgba(40,30,80,.10)` | `rgba(0,0,0,.18)` |
| `accent` par défaut | anneau, boutons pleins | `#5b63e6` | `#8f97ff` |

Les couleurs fixes ne changent pas : `DANGER` `#e5484d`, `LEVEL` `#3fb950`,
icônes des applis.

### Règle des alphas

Le masque du flou derrière nos programmes (ticket 05) part de l'alpha des
pixels. Pour que seules les surfaces de verre soient floutées :

- **scrim et ombres restent sous 0.18** d'alpha ;
- **une surface de verre est au-dessus de 0.45**.

Tout ticket qui choisit une couleur translucide respecte cette règle.

## Les tickets

| N° | Ticket | Dépend de | Fichiers principaux |
|---|---|---|---|
| 03 | [Ombres douces](03-ombres.md) | 01 | `render/shadow.rs`, `render/shaders/shadow.frag`, `render/scene.rs` |
| 04 | [Décorations du compositeur en verre](04-decorations-en-verre.md) | 01, 02 | `decoration/paint.rs`, `render/scene.rs`, `input.rs` |
| 06 | [Dock en verre](06-dock-en-verre.md) | 01 (05 pour voir le flou) | `crates/dock/src/ui/*` |
| 07 | [Paramètres en verre](07-parametres-en-verre.md) | 01 (05 pour voir le flou) | `crates/settings/src/*` |
| 08 | [Documentation et captures](08-documentation-et-captures.md) | tous | `CLAUDE.md`, `docs/maquettes/dock.md` |

### Vagues

1. **01 et 02**, en parallèle.
2. **03, 04, 05, 06 et 07.** 03, 04 et 05 touchent tous
   `crates/compositor/src/render/scene.rs` : un seul agent à la fois, dans
   l'ordre **05 → 04 → 03**. 06 et 07 vont en parallèle avec eux.
3. **08**, en dernier.

## Façon de travailler

- **Un commit sur `main` par ticket**, une fois la CI locale verte
  (`scripts/ci.sh`). Le ticket est supprimé dans ce commit et sa ligne retirée
  du tableau ci-dessus. Ne pas pousser.
- Quand plusieurs agents tournent en parallèle : **un worktree git par
  agent**, puis rebase sur `main` avant de commiter.
- **Vérifier l'API de la version épinglée** de Smithay et d'iced
  (`cargo doc -p smithay`, sources dans `~/.cargo/git/checkouts/smithay-*/19c16d3/`) :
  ne pas se fier à la mémoire.
- Tout ticket qui touche au rendu se vérifie **en imbriqué**, par des captures
  relues **en clair et en sombre**. Le mode d'emploi est dans `CLAUDE.md`
  (« Lancer le compositeur ») : `scripts/debug.sh`,
  `--screenshot fichier.png --screenshot-delay 3000`, `--debug-input`. Une
  capture **avant** le ticket sert de témoin pour la règle cardinale.
- Le backend matériel n'est pas essayable par un agent : la session de Jimmy
  possède les écrans. `cargo run -p coquille -- --probe` compile les shaders sur
  la vraie carte (NVIDIA) et se lance sans risque.
- Code, identifiants et commits en anglais ; textes visibles en français ;
  `cargo fmt` et `cargo clippy -- -D warnings` propres ; logs avec `tracing`.

## Hors périmètre

- Le protocole `ext-background-effect` pour les applis tierces. Smithay le
  fournit (`src/wayland/background_effect`), et un masque « par région »
  s'ajouterait à ceux du ticket 02.
- Les animations : il n'y a pas de système d'animation aujourd'hui.
- Un réglage « Flou » dans l'écran « Paramètres ».
- Toute retouche de **disposition** inspirée des images : centre de contrôle
  en cartes, barre latérale à carrés pastel, rayons plus grands, boutons
  ronds.
