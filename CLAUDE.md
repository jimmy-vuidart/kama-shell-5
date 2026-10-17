# coquille — shell Wayland en Rust (nom provisoire)

Ce fichier donne le contexte du projet à Claude Code. Le garder à la racine du dépôt et le mettre à jour quand une décision change.

## Langue et conventions

- Communiquer avec moi en **français**.
- Code, identifiants, noms de crates et messages de commit en **anglais**.
- Textes visibles par l'utilisateur (barre, lanceur, notifications, menus) en **français**.
- Rust stable, `cargo fmt` et `cargo clippy -- -D warnings` propres, logs avec `tracing`.
- Pas de `unsafe` sauf nécessité documentée par un commentaire.
- **Sauf contre-indication, commiter sur `main` dès qu'une tâche est finalisée** (CI locale verte), sans attendre qu'on le demande. Ne pas pousser sans demande.
- Les API de Smithay et des crates Wayland évoluent vite : **toujours vérifier la documentation de la version réellement utilisée** avant d'écrire du code, et ne pas se fier à la mémoire.

## Le projet en une phrase

Un compositeur Wayland construit sur **Smithay**, pensé pour **un PC de bureau à trois écrans**, utilisé **à la souris**, pour **le jeu et le développement web assisté par IA**, avec un modèle de fenêtres simple : **« Centre + compagnons »**.

## Matériel et environnement

- Distribution : **CachyOS** (base Arch Linux), gestionnaire de paquets `pacman`/`paru`.
- Trois écrans, **tous en paysage** :
  - gauche : 16:9
  - centre : **21:9 ultra-large** (l'écran principal)
  - droite : 16:9
- Souris avec **boutons latéraux** (avant / arrière). Usage majoritairement souris, pas d'un workflow clavier.
- Carte graphique : **NVIDIA RTX 4070 Ti SUPER**, pilote propriétaire (`nvidia_drm modeset=Y`). Les trois écrans y sont branchés : DP-1 (1920×1080), DP-2 (3440×1440, centre), HDMI-A-1 (1920×1080). Un iGPU AMD Radeon 610M existe aussi, sans écran. Conséquences :
  - backend udev : prendre la NVIDIA comme GPU principal, ignorer l'iGPU ;
  - synchronisation explicite (`linux-drm-syncobj`) indispensable ;
  - direct scanout sur plans overlay plus restreint que sur AMD/Intel : à valider, pas acquis ;
  - VRR : d'après `--probe`, seul le centre (DP-2) se dit capable (`vrr_capable`), et le pilote expose l'interrupteur `VRR_ENABLED`.
- Beaucoup de jeux tournent via **Proton / XWayland** : le support XWayland n'est pas optionnel.

## Mes usages (ils guident toutes les décisions)

Je mélange les activités sans prévenir. Exemples réels :

| Situation | Gauche (16:9) | Centre (21:9) | Droite (16:9) |
|---|---|---|---|
| Raid MMO | Discord | Jeu plein écran | Guide web + notes, côte à côte |
| Jeu tranquille | Vidéo (YouTube, TV) | Jeu plein écran | Pages web ou Discord (en onglets) |
| Dev web avec IA | Claude Code (terminal) | Produit web (navigateur) | Éditeur de code |
| Mix | Agent IA + produit web, côte à côte | Jeu plein écran | Discord |

Et parfois des choses inhabituelles (un logiciel de retouche une seule fois, une appli à plusieurs fenêtres…). Le système ne doit jamais m'enfermer.

## Le modèle « Centre + compagnons »

**Le centre, c'est ce que je fais. Les côtés, ce qui m'accompagne.**

### Dispositions par écran

Chaque écran a une disposition parmi quatre, choisie dans « Paramètres » (page Écrans et dock), écrite dans `config.toml` (section `[layout]`) et appliquée à chaud. Une disposition donne le **nombre maximal de colonnes** :

- `full` (« Plein écran ») : une colonne ; les fenêtres rangées sont les onglets d'une même pile.
- `two-columns` (« 2 colonnes ») : jusqu'à deux colonnes, 50/50 par défaut. **Par défaut sur les écrans latéraux.**
- `three-columns` (« 3 colonnes ») : jusqu'à trois colonnes, un tiers chacune par défaut.
- `main-side` (« Maître / esclave ») : une colonne maître à gauche (2/3) et une esclave (1/3).

Le centre est en `full` par défaut. **Les colonnes sont adaptatives** : seules existent celles qui ont des fenêtres (réduites comprises). Une fenêtre rangée seule prend tout l'écran, deux se le partagent, et ainsi de suite jusqu'au maximum ; au-delà, elles s'empilent en onglets. Une colonne qui se vide disparaît, les autres s'élargissent. En `Super + glisser`, un écran qui a encore de la place montre ses colonnes telles qu'elles seraient avec **une de plus, à droite** (libre) ; les colonnes existantes reçoivent la fenêtre en onglet. Changer de disposition ne déplace rien, sauf quand l'écran perd des colonnes : celles en trop rejoignent la dernière, en onglets.

Toutes les séparations se règlent (`Super + glisser` sur la séparation ; chaque colonne garde au moins 20 %). Les largeurs sont gardées **par écran, par disposition et par nombre de colonnes affichées**, d'une session à l'autre (`~/.local/state/coquille/proportions.json`).

« Haut / bas », « Écran libre » et « Deux tiers + un tiers », comme les raccourcis `Super+1` à `5`, ont été retirés : les fenêtres flottantes restent possibles sur tous les écrans.

**Fenêtres maximisées** : chaque écran a aussi une pile « maximisée », posée par-dessus ses colonnes (une colonne pleine, offerte aux écrans partagés sans défaire leur partage). Une fenêtre qui se maximise (double-clic sur sa barre, son bouton « agrandir », celui de sa capsule ou de la barre du shell, demande xdg-shell ou `_NET_WM_STATE` X11) quitte son emplacement et couvre l'écran ; ses capsules forment un groupe en haut au centre, ceux des emplacements restent à leur place, atténués. Cliquer une capsule d'emplacement ramène l'écran partagé (la fenêtre maximisée reste dans son groupe, atténuée) ; cliquer la sienne recouvre l'écran. Réduire la fenêtre maximisée montre la suivante de son groupe, ou l'écran partagé s'il n'y en a plus. Restaurée, elle retourne où elle était (sa colonne, rouverte à sa place si elle s'était fermée et qu'il reste de la place, sinon en onglet ; ou flottante à son rectangle d'avant). Les flottantes restent au-dessus. Une fenêtre dans la seule colonne de son écran a déjà tout l'écran : la demande est sans effet.

### Règles de placement

1. **Une appli jamais rangée s'ouvre en flottant**, sur l'écran où se trouve le curseur, sans déplacer les autres fenêtres. Une petite barre propose : laisser flottante, ranger dans un emplacement, au centre, en onglet.
2. **Glisser une fenêtre par sa barre de titre la déplace** : elle flotte et suit le curseur (une fenêtre rangée quitte son emplacement). **`Super + glisser`**, n'importe où sur la fenêtre, affiche les emplacements possibles sur les trois écrans ; relâcher la place.
3. **Un écran n'est jamais plein** : déposer sur un emplacement occupé empile la fenêtre **en onglet** avec celle qui s'y trouve.
4. **Mémoire par application** : une appli que j'ai rangée moi-même rouvre au même écran / emplacement (clé : `app_id` Wayland ou `WM_CLASS` X11) : dans une colonne rouverte à sa place s'il reste de la place (à côté d'une autre fenêtre de la même appli si elle occupe déjà cette colonne), sinon en onglet dans sa colonne (ou la plus proche). **Les fenêtres laissées flottantes ne créent aucune habitude.** Maximiser est un rangement : l'appli rouvre maximisée sur cet écran, et restaurée elle retourne dans l'emplacement qu'elle avait (ou flotte, et l'habitude s'efface). Ce qu'une appli redemande d'elle-même en s'ouvrant (avant de dessiner ou dans la seconde qui suit : kitty, les applis GTK se souviennent d'avoir été fermées maximisées) est ignoré : c'est la mémoire du shell qui décide.
5. *Supprimée* : l'échange avec le centre (bouton ⇄, `Mod+C`), jugé inutile à l'usage. Il en reste « Au centre », dans la barre de rangement : la fenêtre est rangée au centre, en onglet si l'emplacement est pris, et l'appli y rouvrira.
6. **Instantanés optionnels** : mémoriser la disposition actuelle des trois écrans, la restaurer. Rien n'oblige à préparer quoi que ce soit. *Concept conservé, pas encore réalisé : sa forme reste à définir.*
7. Si je lance une appli alors qu'un jeu est ancré au centre, elle s'ouvre en flottant sur **l'écran latéral utilisé en dernier**, jamais devant le jeu.

### Mode jeu (« écran ancré »)

Quand une fenêtre passe en plein écran (xdg-shell **ou** XWayland) sur un écran, cet écran est **ancré** :

- aucun geste du shell ne s'y applique, aucune décoration, aucun emplacement de dépôt ;
- **affichage direct** (direct scanout sur un plan DRM) quand c'est possible ;
- **VRR** activé si l'écran le supporte ;
- support des protocoles **pointer-constraints** et **relative-pointer** (indispensables aux jeux) ;
- **bords collants** : pour sortir de l'écran du jeu, il faut pousser le curseur contre le bord pendant un court délai (configurable, ~300 ms) ;
- les **notifications** sont redirigées vers un écran latéral ;
- **ne pas déranger** automatique pendant le jeu.

Les deux derniers points sont des concepts conservés, pas encore réalisés : il n'y a pas de notifications pour l'instant. La bascule « Ne pas déranger » du dock existe déjà, sans effet.

Sortie : `Super` libère le pointeur (et ouvre le tiroir du dock) ; `Super + Échap` remet le jeu en fenêtre.

Précisions décidées en phase 3 (à revoir si elles gênent) :

- **Quel écran ?** Celui que l'appli nomme (xdg-shell), sinon celui où se trouve sa fenêtre. Une fenêtre X11 qui démarre en plein écran en couvrant exactement un écran va sur celui-là ; le centre est annoncé à X11 comme écran principal (RandR), ce qui y envoie les jeux Wine/Proton. Un jeu lancé avec le pointeur sur un écran latéral et qui ne dit rien s'ouvre donc là : le ranger une fois au centre suffit, la mémoire par appli fait le reste.
- **`Super` seul** (appuyé puis relâché sans autre touche ni clic) ouvre ou ferme le tiroir du dock, et libère au passage le pointeur du jeu qui le retient, jusqu'au prochain clic dans le jeu. Tant qu'il est libéré, les bords ne collent plus.
- **Aucun panneau sur un écran ancré** : si un jeu prend l'écran gauche, le dock et le tiroir disparaissent, et reviennent quand l'écran est libéré.
- **Bords collants seulement quand le jeu a le focus** : traverser l'écran d'un jeu laissé de côté (focus sur Discord) ne retient pas le curseur. Entrer sur l'écran du jeu n'est jamais retenu. Un glisser du shell n'est jamais retenu.
- **Une fenêtre qui s'ouvre pendant que le jeu a le focus ne lui prend pas le clavier** : elle apparaît sur l'écran latéral (règle 7), sans le focus, avec sa barre de rangement ; un clic dessus le lui donne.
- `Super + Échap` vise le jeu qui a le focus, sinon celui de l'écran du pointeur, sinon le seul jeu ancré.
- « Au centre » disparaît de la barre de rangement tant qu'un jeu tient le centre (ou l'écran de la fenêtre).

### Règle d'or de la souris

**Sans la touche Super, la souris ne fait jamais rien d'autre que ce que fait l'application.** Dans un jeu ancré, tous les boutons appartiennent au jeu.

- `Super + glisser` : ranger une fenêtre (même hors barre de titre) : les emplacements s'affichent, relâcher la place.
- `Super + glisser sur une séparation` : ajuster la largeur des deux colonnes qu'elle sépare.
- **Secouer la souris** : halo autour du curseur pour le retrouver.

## Apparence (référence : maquettes)

Les maquettes sont exportées dans `docs/maquettes/` (les ouvrir avant de toucher au rendu). Celles d'origine, dans `docs/maquettes/shell/`, font référence pour les fenêtres, leurs décorations et le rangement ; **plus pour les barres latérales ni pour le lanceur**, abandonnés sous cette forme.

- Style proche de Windows 11 / Fluent : coins arrondis, surfaces translucides en **verre liquide** flouté, sobre.
- Thèmes **clair** et **sombre** (anthracite translucide).
- Fenêtres : rayon **12 px**, espacement **8 px** entre emplacements et avec les bords.
- **Règle cardinale de la refonte** : aucune disposition, métrique, taille ou espacement ne change. Seules les matières (surfaces translucides, flou, liserés rim et ombres douces) changent.
- **Décorations côté serveur** (dessinées par le compositeur, via `xdg-decoration`) :
  - barre de titre **38 px** : verre (`palette.glass`) avec liseré clair (`palette.rim`), floutée par le compositeur, icône et nom de l'appli (ceux de son fichier `.desktop` ; à défaut un carré de couleur et un nom tiré de son identifiant), sous-titre atténué ;
  - boutons à droite, pour une fenêtre flottante seulement : réduire, agrandir, fermer (une fenêtre rangée ou maximisée les a sur sa capsule) ;
  - un double-clic sur la barre maximise la fenêtre, un autre la restaure.
- **Pas de double barre** : une fenêtre qui dessine déjà sa barre de titre (applis GTK comme Nautilus, qui ne demandent pas `xdg-decoration` ; applis Electron comme Discord ou Claude Desktop, qui demandent par `xdg-decoration` à se décorer elles-mêmes, et sont alors prises au mot ; applis X11 qui refusent la décoration par leurs indications Motif, comme Steam) ne reçoit jamais celle du shell, même dans un emplacement partagé. Glisser sa propre barre vaut glisser celle du shell (la fenêtre flotte et suit le curseur).
- **Capsules** : les onglets ne sont plus dans les barres de titre. Chaque emplacement occupé porte au-dessus de lui (bande de **32 px**, puis 8 px) un groupe de capsules centré, une par fenêtre de sa pile, même s'il n'y en a qu'une : icône et nom de l'appli. La capsule de la fenêtre affichée est pleine (`palette.glass_strong` avec liseré `palette.rim`) et montre réduire, agrandir (« restaurer », deux carrés, dans le groupe des fenêtres maximisées) et fermer ; les autres les montrent au survol, à une place réservée (survoler ne déplace rien). Fenêtre réduite : capsule grisée, qui reste même si tout l'emplacement est réduit. Capsule de la fenêtre active : anneau d'accent. Quand la place manque, les plus larges rétrécissent d'abord, jusqu'à l'icône seule. Les fenêtres maximisées ont leur groupe en haut au centre de l'écran (dans la moitié centrale de la bande) ; un groupe d'emplacement qui le toucherait s'écarte du côté de son milieu (à gauche s'ils partagent le même). Ni capsule pour une fenêtre flottante, ni sur un écran ancré. Spécification : `docs/superpowers/specs/2026-10-12-capsules-design.md`.
- Fenêtre active : **anneau d'accent de 2 px**, d'une couleur unie. Accent par défaut : pervenche `#5b63e6` (clair), `#8f97ff` (sombre), réglable (`accent`, page Apparence de « Paramètres »).
- **Ombres douces** : seules les fenêtres flottantes et la barre de rangement projettent une ombre douce teintée (`palette.shadow`), dessinée par le shader `shadow` (`render/shadow.rs`). Une fenêtre rangée n'en a pas : à 8 px, elle salirait sa voisine et les capsules. L'ombre est évidée sous la fenêtre qui la projette, pour qu'une barre de titre translucide montre ce qui est derrière, pas l'ombre.
- **Flou d'arrière-plan** : le compositeur calcule un vrai flou d'arrière-plan (« framebuffer effects » de Smithay, Dual Kawase : passes de réduction puis d'agrandissement, shaders `blur_down` et `blur_up`, puis `glass` qui applique le masque) derrière ses décorations et derrière les programmes du shell (panneaux `coquille-*`, fenêtre `coquille-settings`). Le verre d'une barre de titre est sous la fenêtre : il floute ce qui est derrière elle, jamais son contenu. Coupé en mode jeu (écran ancré).
- **Règle des alphas** : derrière nos programmes, le flou prend la forme de leurs pixels assez opaques (couverture `smoothstep(0.2, 0.4, alpha)`). Les surfaces de verre ont donc un alpha > 0.45, le voile/scrim et les ombres restent sous 0.18 (vérifié par un test de `crates/theme`). Le fond de fenêtre (`surface`), visible là où une appli n'a pas encore dessiné, reste opaque.
- **Un accent or métallique a été essayé, puis abandonné** : dégradés, lingots, filets et gris chauds n'étaient pas jolis à l'usage (commits `18374c1` à `1ad7eaa`, annulés). Ne pas y revenir sans que je le demande.
- Les barres latérales des maquettes d'origine sont abandonnées. Le **dock** (maquettes `docs/maquettes/dock/`, cahier des charges `docs/maquettes/dock.md`) a une taille d'icônes configurable (`[dock]` `icon_size`, défaut 24 px ; barre de 44 px à 8 px du bord et zone réservée de 60 px par défaut, qui s'ajustent à la taille choisie). Chaque écran peut avoir la sienne (`[dock.screens]`), dans un sens ou dans l'autre : **verticale** sur le bord gauche ou sur le bord droit (elle se déploie au survol, lanceur en tiroir du même côté ; à droite, tout est en miroir), ou **horizontale** sur le bord bas (étiquette au survol, lanceur en panneau centré au-dessus). Par défaut : une barre verticale sur l'écran gauche.
- Police d'interface : Figtree (repli : police système sans-serif).
- **Icônes des applis : thème Reversal**, carrés arrondis comme sous macOS, sans détourner les marques (paquet AUR `reversal-icon-theme-git`, qui n'installe que la variante par défaut, `Reversal` et `Reversal-dark`). Le dock et le compositeur prennent la variante claire ou sombre selon le thème, et la couleur la plus proche de l'accent si sa variante est installée (`Reversal-purple-dark`…), sinon la variante par défaut ; sans Reversal, le thème d'icônes de GNOME (`gsettings`). Le choix (`crates/apps/src/reversal.rs`) suit les changements de thème et d'accent en cours de session. Les applis lancées, elles, gardent le thème d'icônes du système.

## Architecture visée

Workspace Cargo :

```
coquille/
├─ crates/
│  ├─ model/        # logique pure : écrans, dispositions, emplacements, règles, mémoire.
│  │                # AUCUNE dépendance Wayland. Entièrement testé unitairement.
│  ├─ ipc/          # ce que le compositeur et le dock se disent (JSON par lignes).
│  ├─ apps/         # les applis installées (fichiers .desktop, icônes Reversal).
│  ├─ config/       # le fichier de configuration : le lire, en réécrire un réglage à la fois.
│  ├─ theme/        # jetons d'apparence purs (couleurs, accents, repli), sans dépendance graphique.
│  ├─ ui/           # ce que partagent les programmes du shell dessinés avec iced :
│  │                # couleurs, police Figtree, symboles, lien au compositeur.
│  ├─ compositor/   # binaire `coquille` : Smithay, rendu, entrées, décorations, mode jeu.
│  ├─ dock/         # binaire `coquille-dock` : barre, tiroir, centre de contrôle, mixeur.
│  └─ settings/     # binaire `coquille-settings` : l'écran « Paramètres ».
├─ vendor/          # code tiers porté dans le dépôt (voir vendor/README.md).
├─ docs/maquettes/
└─ CLAUDE.md
```

- La crate `model` est le cœur : toute règle de placement doit y vivre et y être testée, le compositeur ne fait qu'appliquer ses décisions.
- Le shell est fait de **programmes séparés**, reliés au compositeur par un socket Unix (crate `ipc`) et écrits avec **iced**. Le dock est le premier, affiché par layer-shell (`iced_layershell`). L'écran « Paramètres » est le deuxième : une fenêtre ordinaire, que le compositeur traite comme n'importe quelle appli. Notifications et instantanés n'ont pas encore de forme.
- Configuration : `~/.config/coquille/config.toml` (identification des écrans par connecteur et/ou EDID, accent, thème, disposition de chaque écran, délai des bords collants).
- État persistant (mémoire par appli, largeurs des colonnes, instantanés) : `~/.local/state/coquille/`.

### Trouver son chemin dans le code

`AGENTS.md` est un lien vers ce fichier : les agents qui ne lisent pas `CLAUDE.md` (Antigravity, Gemini, Codex) partent du même contexte. Ne modifier que `CLAUDE.md`.

Avant de planifier une fonctionnalité, **interroger le graphe du code plutôt que lire les crates fichier après fichier**. [codegraph](https://github.com/colbymchenry/codegraph) (version épinglée dans `scripts/codegraph.sh`) indexe les symboles Rust et leurs appels, d'une crate à l'autre :

```
scripts/codegraph.sh explore "how a setting is reloaded while running"   # le code d'un sujet, et ce qui en dépend
scripts/codegraph.sh callers apply_arrangement                           # qui appelle
scripts/codegraph.sh callees manage                                      # ce qui est appelé
scripts/codegraph.sh impact Ratio                                        # ce qu'un changement touche
scripts/codegraph.sh files                                               # l'arbre des fichiers indexés
```

- Les agents qui parlent MCP ont la même chose par l'outil `codegraph_explore` : `.mcp.json` pour Claude Code, `opencode.json` pour OpenCode, `agy mcp list` pour Antigravity. Le serveur suit les fichiers modifiés ; le script, lui, met l'index à jour avant chaque question.
- L'index est dans `.codegraph/`, jamais commité, construit au premier appel (moins d'une seconde). Rien ne quitte la machine, télémétrie coupée.
- Poser les questions **en anglais**, comme les identifiants. Chaque fichier commence par un commentaire `//!` qui dit son rôle : c'est ce que la recherche retrouve.
- Limites : le graphe s'arrête au dépôt. Une méthode que Smithay appelle (les gestionnaires de `handlers/`, comme `new_toplevel`) y figure sans appelant, et le code de `vendor/` n'y est pas. Pour ce qui vient de Smithay, `cargo doc` et ses exemples.

### Décisions techniques

- **Smithay est épinglé sur un commit git** (le même que niri, voir `Cargo.toml`), pas sur crates.io. Sa documentation se lit avec `cargo doc -p smithay --open` ; les exemples `anvil` et `smallvil` sont dans `~/.cargo/git/checkouts/smithay-*/`.
- **Décorations et verre en Rust pur / GLES** : `tiny-skia` (formes) et `cosmic-text` (texte) vers des textures, shaders GLES pour l'anneau, le rognage des coins, l'ombre douce (`render/shadow.rs`) et le flou (`render/glass.rs`, framebuffer effects de Smithay). Le flou fait des appels GL bruts par `frame.with_context` (`unsafe` documenté à chaque bloc). Pièges, tous rencontrés :
  - ne jamais rebrancher le framebuffer 0 : il n'existe pas sur le matériel (GBM), seulement en imbriqué. Lire la cible courante au début, tout remettre à la fin ;
  - tout placer d'après `gl_FragCoord` et la projection du rendu (`window_rect`), jamais en devinant le sens vertical de la cible ;
  - le masque alpha d'un client est dessiné par le rendu de Smithay lui-même (il connaît les tampons externes, inversés, recadrés), redessiné seulement quand la surface est validée ;
  - une texture que le rendu n'a jamais échantillonnée garde le filtre par défaut de GL, qui attend des mipmaps : sans `TEXTURE_MIN_FILTER`, elle se lit opaque partout ;
  - les textures sont réservées par paliers de 64 px et réutilisées ; un framebuffer et un quad partagés vivent dans le contexte EGL ;
  - chaque écran est dessiné à part sur le matériel : un cache d'élément (verre, ombre, forme) est tenu par vue (`render/per_view.rs`), sinon il change à chaque écran dessiné et tout est refloué ;
  - la marge capturée autour du verre est la portée réelle du flou (`blur_reach`, 32 px à la force 3).
- **Mode imbriqué (winit) : `Alt` remplace `Super`**, que la session hôte intercepte. `scripts/ci.sh` lance `fmt`, `clippy` et les tests.
- **La donnée de la boucle d'événements est `State` lui-même** (plus de `App { display, state }`) : le gestionnaire de fenêtres X11 de Smithay l'exige. Le `Display` Wayland appartient à la source qui distribue les requêtes des clients (`main.rs`), toujours sans `unsafe`.
- **Focus clavier = `KeyFocus`** (`focus.rs`) : la surface Wayland, accompagnée de la fenêtre X11 quand c'en est une, pour que le serveur X soit prévenu des changements de focus. Le focus du pointeur reste une `WlSurface`.
- **XWayland** (`xwayland.rs`) : démarré avec le compositeur par Smithay, qui fait aussi le gestionnaire de fenêtres X11. Piège de cette version de Smithay : après `set_randr_primary_output`, il faut une autre requête X11 tout de suite, sinon **tous** les événements X11 suivants sont ignorés (voir le commentaire dans `xwayland_ready`).
- **Décisions pures du mode jeu** dans `game.rs` (testées) : tap de `Mod`, confinement, bords collants, raccourcis matériels d'un écran (`Boost`). Les règles de placement du mode jeu restent dans `model`.

- **Panneaux (layer-shell)**, `panels.rs` : une surface layer-shell n'est pas une fenêtre (ni barre de titre, ni emplacement, ni mémoire). Sa zone exclusive alimente `OutputInfo::reserved`. Ordre de dessin et de clic : calque `Overlay`, barre de rangement, menus, calque `Top`, fenêtres, calques `Bottom` et `Background`. Clavier : un panneau `Exclusive` au-dessus des fenêtres l'a tant qu'il est affiché (le tiroir), un panneau `OnDemand` quand on clique dessus, le dock jamais ; il revient ensuite à la fenêtre qui l'avait.
- **Canal du shell**, `ipc.rs` : socket `$XDG_RUNTIME_DIR/coquille-<affichage Wayland>.sock`, annoncé par `COQUILLE_SOCKET` aux programmes lancés. L'état entier (écrans, fenêtres, habitudes, thème) est poussé à chaque changement ; les commandes sont dans `crates/ipc`. Un message mal formé est journalisé et ignoré.
- **winit et iced** : `iced_layershell` est écrit pour winit-core 0.31.0-beta.2, Smithay épingle la beta.3. `vendor/iced_exdevtools` (une ligne modifiée) les réconcilie par `[patch.crates-io]` ; à retirer quand `iced_layershell` suivra.
- **Le dock passe par les outils des services** (`pactl`, `parec`, `nmcli`, `bluetoothctl`, `ddcutil`) plutôt que par leurs bibliothèques : rien à lier, un outil absent grise sa commande.
- **Thème et accent** : `theme` et `accent` de la configuration valent pour le compositeur et le dock ; le thème change en cours de session (commande `set_theme`).
- **Configuration relue à chaud** : la commande `reload_config` fait relire `config.toml` au compositeur (`reload.rs`), qui applique ce qui peut changer en cours de route (thème, accent, terminal, disposition du clavier, disposition de chaque écran, bords collants, affichage direct et VRR) puis prévient les autres programmes (`config_changed`) : le dock relit alors sa part et déplace ses barres. **Le rôle des écrans et XWayland ne changent qu'au démarrage.** Un fichier illisible ou invalide ne change rien.
- **Un seul endroit écrit le fichier** : la crate `config` (`toml_edit`). Elle remplace une valeur là où elle est, commentaires conservés ; un réglage que le fichier ne fait que suggérer (`# vrr = true`) est activé sur place, sous son explication. Le compositeur, lui, lit le fichier strictement (`deny_unknown_fields`) : un test vérifie qu'il accepte tout ce que l'écran « Paramètres » écrit.

### Lancer le compositeur

Deux backends, choisis tout seuls (`--backend auto`) : **imbriqué** (winit) s'il y a déjà une session graphique, **matériel** (udev) sinon.

```
cargo run -p coquille -- --three-screens --spawn kitty      # imbriqué, trois écrans simulés
cargo run -p coquille -- --role left --spawn kitty          # imbriqué, un seul écran
cargo run -p coquille -- --probe                            # examine GPU et écrans, n'affiche rien
```

- Le plus rapide : `scripts/debug.sh` compile (debug) et lance coquille et le dock en imbriqué. `scripts/debug.sh left` pour un seul écran, `-f` en plein écran, `--mock` pour un dock simulé, `-- <options de coquille>` pour le reste. Les boutons de session du dock y sont simulés.
- Souris : glisser une barre de titre déplace la fenêtre librement (rangée, elle devient flottante et reprend la taille que choisit l'appli, le curseur restant sur sa barre) ; `Mod+glisser` sur une fenêtre, ou glisser une capsule, montre les emplacements des trois écrans, relâcher range la fenêtre (en onglet si l'emplacement est pris ; relâcher ailleurs ne change rien). `Mod+clic` sans bouger ne fait rien. Une fenêtre qui s'ouvre flottante est accompagnée d'une petite barre : « Laisser flottante », « Ranger ici » / « En onglet », « Au centre ».
- Souris, suite : tirer un bord ou un coin d'une fenêtre flottante la redimensionne (le compositeur impose alors sa taille) ; `Mod+glisser` sur la séparation de deux colonnes règle leurs largeurs.
- Capsules : un clic affiche la fenêtre ; « réduire » la cache (sa capsule reste, grisée ; un clic dessus, sur son icône du dock, ou `Mod+R` la ramène) ; « agrandir » la maximise (« restaurer » la remet à sa place) ; « fermer » la ferme, affichée ou non. La barre de titre d'une fenêtre flottante garde ces trois boutons ; un double-clic sur une barre de titre du shell maximise ou restaure.
- Dock : `target/debug/coquille --role left --spawn "$PWD/target/debug/coquille-dock --mock"` (tout simulé) ; sans `--mock`, vraies fenêtres et vrais services. Détail dans `crates/dock/README.md`.
- Raccourcis (touche Mod = `Super` sur le matériel, `Alt` en imbriqué) : `Mod+Entrée` terminal ; `Mod+Page suiv./préc.` onglet suivant/précédent ; `Mod+R` restaure la dernière fenêtre réduite ; `Mod+Maj+E` quitter.
- Mode jeu : `Mod+Échap` remet le jeu en fenêtre ; `Mod` seul ouvre le tiroir du dock et libère le pointeur. (En imbriqué, GNOME garde `Alt+Échap` pour lui : ce raccourci ne s'essaie qu'avec `--debug-input`.)
- Applications X11 : `DISPLAY` est transmis aux programmes lancés par le compositeur (et aux services de la session avec `--export-environment`). `--no-xwayland` ou `xwayland = false` s'en passent.
- Mémoire par appli : enregistrée dans `~/.local/state/coquille/memory.json` sur le matériel ; en imbriqué, seulement avec `--state-dir`.
- Sorties de secours, sans Mod : `Ctrl+Alt+Retour arrière` quitte, `Ctrl+Alt+F1…F12` change de terminal virtuel, `--exit-after <secondes>` quitte tout seul.
- Vérifier le rendu sans regarder l'écran (imbriqué) : `--screenshot fichier.png --screenshot-delay 3000` enregistre la sortie puis quitte ; `--debug-input "2500 move 400 450; 100 click; 300 key alt+1"` rejoue des entrées (syntaxe dans `crates/compositor/src/debug.rs`) et ignore alors la vraie souris. Un glisser s'écrit `down`, `move…`, `up` ; une touche tenue, `press alt` … `release alt` ; `rclick`, `mclick` et `wheel 3` (ou `-1`) rejouent le clic droit, le clic milieu et la molette. Placer le pointeur dès `0 move x y` : une fenêtre s'ouvre sur l'écran où il se trouve à ce moment-là. `rel dx dy` déplace le pointeur comme une vraie souris (chemin du backend matériel : pointeur verrouillé, bords collants, mouvement relatif), alors que `move` le pose comme le fait la fenêtre hôte.
- Clients de test utiles en imbriqué : `kitty --start-as fullscreen` (plein écran Wayland), `kitty -o linux_display_server=x11` et `glxgears` (X11), `GDK_BACKEND=x11 gnome-calculator` (menus X11). Un client SDL2 (`SDL_VIDEODRIVER=wayland` ou `x11`, `SDL_SetRelativeMouseMode`) verrouille le pointeur ; `libSDL2` est là, quelques lignes de Python `ctypes` suffisent.
- `--probe` exerce sur la vraie carte tout ce qui ne demande pas de posséder les écrans : EGL sur GBM, moteur de rendu, shaders, détection et rôles des écrans, capacité VRR de chacun. À lancer après tout changement du backend matériel.

- Paramètres : le bouton du dock (à côté de la déconnexion) lance `coquille-settings` sur le fichier que lit le dock. À la main : `target/debug/coquille-settings --config FICHIER --page screens` (`appearance`, `screens`, `game`, `system`). Détail dans `crates/settings/README.md`.

Réglages de `config.toml` (modèle commenté : `config/config.toml` ; les réglages généraux doivent précéder les sections) : `theme`, `accent`, `terminal`, `xwayland`, `[glass]` (`blur` = `true`, `strength` de 1 à 5, défaut `3` ; un entier, sinon tout le fichier est refusé), `[outputs]`, `[layout]` (`left`, `center`, `right` : `"full"`, `"two-columns"`, `"three-columns"` ou `"main-side"`), `[keyboard]`, et pour le mode jeu `[game]` : `sticky_edges_ms` (300 ; 0 désactive), `direct_scanout` (`true`), `vrr` (`true`). Les sections `[dock]` (`favorites`, `icon_size`), `[dock.screens]` (`left`, `center`, `right` : `"vertical"`, `"vertical-right"`, `"horizontal"` ou `"off"`) et `[drawer]` (`pinned`, `width`, `height`) sont lues par le dock, qui réécrit `favorites`, `theme`, `width` et `height` quand on les change à la souris (le lanceur se redimensionne en tirant sa poignée).

### Installer

`scripts/install.sh` compile en release et installe **pour l'utilisateur** : `~/.local/bin/coquille`, `~/.local/bin/coquille-dock`, `~/.local/bin/coquille-session` (lanceur de session), `~/.local/bin/coquille-dock-watch` (lance le dock et le relance s'il tombe), `~/.local/bin/coquille-settings` et son entrée de menu `coquille-settings.desktop` (qui donne son nom et son icône à la fenêtre), une entrée de menu (imbriqué), et `~/.config/coquille/config.toml` s'il n'existe pas (modèle : `config/config.toml`). Il vérifie aussi que le thème d'icônes Reversal est là et, sinon, donne la commande pour installer le paquet (`paru -S reversal-icon-theme-git`) sans l'installer lui-même. À terme, coquille s'installera comme paquet AUR qui en dépendra (ticket 25).

Le dock démarre avec le compositeur, dans la session comme depuis l'entrée de menu. `COQUILLE_NO_DOCK=1` s'en passe ; `COQUILLE_DOCK_ARGS` lui passe des options. Ses boutons Se déconnecter, Redémarrer et Éteindre sont **réels** (après confirmation) partout sauf avec `--mock` : en imbriqué aussi, Redémarrer redémarre la machine.

`scripts/install.sh --session` ajoute en plus deux sessions à l'écran de connexion (`/usr/share/wayland-sessions/`, seul endroit qui demande `sudo`) : « coquille » et « coquille (essai, 60 secondes) ». La session par défaut n'est jamais modifiée. `scripts/install.sh --uninstall` retire tout.

Journaux : `~/.local/state/coquille/session.log` (et `session.previous.log` pour le démarrage d'avant), paniques comprises. Ce que dit le serveur X va à côté, dans `xwayland.log`, et ce que dit le dock dans `dock.log` (et `dock.previous.log`).

### État de la phase 2

Terminée : trois écrans réels, flottant par défaut à la taille choisie par l'appli, glisser-déposer vers les emplacements, onglets (depuis en capsules au-dessus des emplacements), mémoire par appli, écran libre, barre de rangement, redimensionnement des flottantes, ratio à la souris, bouton « réduire ». L'échange avec le centre (⇄), la disposition « Onglets », puis l'écran libre et les raccourcis `Mod+1` à `5` ont été retirés depuis (dispositions adaptatives, voir plus bas).

La disposition d'un écran se change dans « Paramètres ».

### État de la phase 3

Fait, **vérifié en imbriqué** (essais scriptés, captures relues) et par tests unitaires :

- plein écran = écran ancré, pour xdg-shell et X11 (demande avant ou après l'ouverture, sortie par l'appli, par `Mod+Échap`, ou parce que la fenêtre disparaît) ; règle 7 ; pas de décoration, de geste ni d'emplacement sur l'écran ancré ;
- pointer-constraints (verrouillage et confinement) et relative-pointer, avec un client SDL Wayland et un client SDL X11 ; `Mod` seul libère le pointeur, un clic le rend ;
- bords collants (essayés avec `rel`) ;
- XWayland : fenêtres X11 gérées comme les autres (flottantes à la taille demandée, barre de titre, emplacements, onglets, mémoire par `WM_CLASS`, fermeture), menus et infobulles (fenêtres « override-redirect ») affichés et cliquables, presse-papiers partagé dans les deux sens, résolutions émulées (viewporter).

Fait mais **jamais exécuté** (backend matériel seulement, relu et compilé) :

- mouvement relatif venant de libinput (deltas accélérés et bruts) et bords collants sur le vrai mouvement : le code est celui qu'exerce `rel`, seule la source des événements diffère ;
- **affichage direct** : sur un écran ancré seulement, plan principal seulement. L'image du jeu part telle quelle tant que rien d'autre n'est sur son écran (le pointeur reste dessiné par le compositeur : dès qu'il est visible sur le jeu, l'image est composée). Un échec fait redessiner l'image composée et coupe l'affichage direct de cet écran jusqu'à la fin de la session ;
- **VRR** : activé quand un jeu prend l'écran, coupé quand il le quitte, là où le connecteur l'annonce utilisable sans changement de mode (le centre). Un refus du pilote ou une image qui échoue le coupe pour de bon ;
- **flou et ombres en matériel (verre liquide)** : coût du flou sur les trois écrans réels (notamment le 3440×1440 ultra-large au centre) ; affichage direct toujours obtenu en jeu (`direct scanout: the buffer of the game is shown as it is` dans `session.log`). Une première version perdait des images sur le matériel (`BlitFramebuffer error` dans `session.log`, le 16 octobre) ; corrigée, vérifiée en imbriqué et par le chemin de capture hors écran, pas encore sur les vrais écrans : le journal ne doit plus contenir ni `BlitFramebuffer error` ni `glass: GL error`. `--probe` compile les shaders du verre sans erreur sur la RTX 4070 Ti SUPER ;
- le journal dit, pour chaque écran allumé, ce qu'il sait faire (`vrr=…`), puis `variable refresh rate switched`, et une fois par jeu `direct scanout: the buffer of the game is shown as it is` ou `direct scanout not possible for this frame: composited`. Pour les raisons détaillées du pilote, lancer depuis un TTY avec `RUST_LOG=info,smithay::backend::drm::compositor=trace` (très bavard : avec `--exit-after`).

Limites connues :

- une appli qui dessine sa propre barre peut être glissée par elle (demande de déplacement xdg-shell ou `_NET_WM_MOVERESIZE`), mais ses demandes de redimensionnement sont ignorées : une fenêtre flottante se redimensionne par les bords du shell ;
- ne jamais appeler `pointer.unset_grab` depuis un gestionnaire de requête client (`move_request`…) : le compositeur se bloque ;
- une fenêtre X11 sans décoration qui couvre l'écran sans demander le plein écran (`_NET_WM_STATE_FULLSCREEN`) n'est pas reconnue comme un jeu ;
- pas de sélection primaire (clic du milieu) ni de glisser-déposer entre X11 et Wayland ; pas de `xwayland-keyboard-grab` ;
- pas de retour dmabuf « scanout » par surface : l'affichage direct dépend de ce que le jeu alloue de lui-même ;
- avec le VRR, un pointeur visible qui bouge fait remonter la fréquence (il n'est pas sur un plan matériel) ;
- bords collants inopérants en imbriqué avec la vraie souris : l'hôte possède le pointeur, rien ne peut le retenir ;
- notifications redirigées et « ne pas déranger » : concepts conservés, à concevoir.

### État de la phase 4 (dock)

Fait, **vérifié en imbriqué** (essais scriptés, captures relues) et par tests unitaires :

- panneaux layer-shell dans le compositeur : zone réservée, menus (popups), clavier à la demande, retrait sur un écran ancré ;
- canal du shell : état poussé, commandes (activer, fermer, ouvrir sur un écran, oublier une place, basculer le tiroir, changer de thème, quitter) ;
- règles du modèle : ouvrir une fois sur un écran sans toucher à la mémoire, oublier une place, jeux au centre (classe `steam_app_*`) ;
- dock par écran, vertical (bord gauche ou droit) ou horizontal ; `Super` ouvre le lanceur sur l'écran du pointeur ;
- dock : barre (favoris, puis toutes les applis ouvertes des trois écrans, indicateurs, heure), déploiement au survol, clics, menu contextuel, réordonnancement ; tiroir (recherche, épinglées, applications paginées, jeux Steam) ; centre de contrôle et mixeur ; favoris et thème réécrits dans `config.toml`.

Fait mais **jamais exercé pour de vrai** : couper le réseau ou le Bluetooth depuis le centre de contrôle (les états sont lus, les bascules n'ont pas été actionnées), la luminosité (`ddcutil` n'est pas installé), Redémarrer et Éteindre (seule la déconnexion a été essayée, en imbriqué).

Intégration à la session : écrite, **essayée en imbriqué et dans un dossier d'essai seulement** (`scripts/install.sh` avec `PREFIX`, `SESSIONS_DIR` et les dossiers XDG redirigés ; `coquille-dock-watch` relance bien un dock tué et s'arrête avec le compositeur). Jamais lancée depuis l'écran de connexion. Si le dock ne s'affiche pas sur le matériel (il dessine avec le GPU, par wgpu), `ICED_BACKEND=tiny-skia` le fait dessiner sans.

Pas fait : les messages non lus réels ; glisser une icône du tiroir vers un écran. Limites du dock : `crates/dock/README.md`.

### État de l'écran « Paramètres »

Fait, **vérifié en imbriqué** (essais scriptés, captures relues en clair et en sombre) et par tests unitaires : cinq pages (Apparence, Écrans et dock, Mode jeu, Souris, Système) ; chaque changement est écrit aussitôt dans `config.toml` et appliqué à chaud (thème, accent, disposition et sens du dock par écran, vitesse du pointeur, profil d'accélération, défilement naturel, inversion des boutons, disposition du clavier) ; le thème changé depuis le dock se reflète dans la fenêtre ; hors de coquille, la fenêtre règle le fichier pour le prochain démarrage.

Fait mais **jamais exécuté** (backend matériel) : le choix du connecteur de chaque écran (en imbriqué les écrans sont simulés, la liste est remplacée par « Écrans simulés » ; l'échange de deux connecteurs a été essayé sans compositeur et par tests) ; l'affichage direct et le VRR coupés ou remis en cours de session (`Udev::set_game`) ; les réglages libinput appliqués aux périphériques pointeur (`Udev::set_mouse`).

Limites connues :

- pas de réglage pour les favoris du dock, les épinglées et la taille du lanceur : ils se règlent à la souris dans le dock ;
- les dispositions du clavier portent leur nom anglais (celui de XKB) ;
- la disposition donnée par `--keyboard-layout` l'emporte sur le fichier : la changer dans l'écran est alors sans effet jusqu'au prochain démarrage sans l'option ;

### État des capsules

Fait, **vérifié en imbriqué** (essais scriptés, captures relues en clair et en sombre) et par tests unitaires : groupes par emplacement, clic, survol, réduire, restaurer, fermer une fenêtre affichée ou non, glisser une capsule vers un autre emplacement, groupes qui suivent le ratio, disparition sur un écran ancré, « Au centre ». Jamais vu sur les trois vrais écrans.

Limites connues :

- deux fenêtres de la même appli dans un emplacement ont des capsules identiques (le nom de l'appli, pas le titre de la fenêtre) ;
- pas de réordonnancement des capsules à la souris ;
- au-delà de ce que la bande peut montrer en icônes seules, les dernières capsules sont coupées.

### État des fenêtres maximisées

Fait, **vérifié en imbriqué** (essais scriptés, captures relues en clair et en sombre) et par tests unitaires (modèle : pile maximisée, recouvrement, retour à la place d'avant, mémoire ; compositeur : géométrie des capsules et de l'écart entre groupes, double-clic) : demande xdg-shell (double-clic et bouton d'une appli GTK), demande X11 (bouton d'une appli GTK sous XWayland), double-clic sur la barre du shell, boutons « agrandir » de la barre d'une flottante et des capsules, retour au partage par une capsule d'emplacement, recouvrement par la capsule maximisée, restauration dans la colonne, réouverture maximisée par la mémoire, demandes faites à l'ouverture ignorées (kitty, éditeur GTK), groupe d'emplacement écarté du groupe maximisé en `Full`. Jamais vu sur les trois vrais écrans.

Limites connues :

- pas de raccourci clavier ni de zone de dépôt « maximisée » : on maximise depuis la fenêtre ou sa capsule ;
- une appli qui redemande son état d'avant plus d'une seconde après avoir dessiné est prise au mot ;
- le canal du shell ne dit pas qu'une fenêtre ou une habitude est maximisée (le dock n'en a pas l'usage).

### État des dispositions adaptatives

Fait, **vérifié en imbriqué** (essais scriptés, captures relues) et par tests unitaires : quatre dispositions par écran, choisies dans « Paramètres » et relues à chaud (passer de 3 à 2 colonnes empile la troisième en onglet dans la deuxième) ; 1, 2, 3 fenêtres rangées → tout l'écran, moitiés, tiers, la suivante en onglet ; colonne fermée → les autres s'élargissent ; zones de dépôt avec la nouvelle colonne à droite ; séparations des 3 colonnes réglées à la souris et retrouvées à la session suivante. Jamais vu sur les trois vrais écrans.

Limites connues :

- les colonnes d'un écran se renumérotent quand l'une se ferme : une appli mémorisée « colonne 3 » rouvre dans la plus proche disponible ;
- une nouvelle colonne s'ouvre toujours à droite en `Super + glisser` (la mémoire, elle, rouvre une colonne à sa place) : pour réordonner, on déplace une fenêtre ;
- la colonne maître est toujours à gauche.

### Backend matériel : ce qui est fait et ce qui ne l'est pas

- Un seul GPU : celui du démarrage (la NVIDIA). Écrans détectés **au démarrage seulement** (pas de branchement à chaud).
- Rôles des écrans : `[outputs]` du fichier de configuration, sinon le plus large au centre. Placés côte à côte, alignés en haut.
- Tout est composé par le GPU, sauf l'écran d'un jeu (affichage direct et VRR, voir l'état de la phase 3). Pas de plan matériel pour le curseur, pas de plans overlay. Synchronisation explicite activée quand le pilote la propose.
- Je (Claude) ne peux pas tester ce backend moi-même : la session en cours possède les écrans. Seuls `--probe` et le mode imbriqué sont vérifiables sans l'utilisateur.

### Références à étudier avant de coder

- **anvil**, le compositeur d'exemple du dépôt Smithay : structure de base, backends winit et udev.
- **niri** (Rust, Smithay) : multi-écrans, direct scanout, VRR, XWayland, finitions.
- **cosmic-comp** (Rust, Smithay) : tiling et gestion de plusieurs écrans.

## Feuille de route

On avance **par phases**, en s'arrêtant à la fin de chacune pour que je teste.

- **Phase 0 — Socle** : workspace, crate `model` avec types et tests (dispositions, emplacements, placement, onglets, mémoire), CI locale (`fmt`, `clippy`, `test`).
- **Phase 1 — Compositeur imbriqué** : backend **winit** (fenêtre dans ma session actuelle), xdg-shell, une sortie simulée, dispositions `Full` / `Columns`, décorations serveur, focus et anneau d'accent. Test avec `foot` ou `alacritty`.
- **Phase 2 — Trois écrans réels** : backend **udev** (DRM/KMS, GBM, libinput, libseat), trois sorties dont le 21:9 central, glisser-déposer vers les emplacements, flottant par défaut, onglets, ⇄, mémoire par appli, `Free`.
- **Phase 3 — Jeu** : XWayland, détection du plein écran, écran ancré, direct scanout, VRR, pointer-constraints, relative-pointer, bords collants.
- **Phase 4 — Shell** : le **dock** (barre, tiroir, centre de contrôle, mixeur), d'après `docs/maquettes/dock.md`. Reste son intégration à la session, puis les notifications et les instantanés, dont la forme n'est pas définie. Abandonnés : les barres latérales et le lanceur des maquettes d'origine, le saut de curseur au bouton latéral. Détail dans `docs/tasks/PHASE_4.md`.
- **Plus tard** : calque par-dessus le jeu (performances, son par appli, capture), vignettes vivantes.

## Sécurité pendant le développement

- Tout développer et tester d'abord en **mode imbriqué (winit)**, dans ma session actuelle.
- Ne jamais remplacer ma session par défaut ni modifier le gestionnaire de connexion (GDM) sans me le demander.
- Pour les tests sur TTY : prévoir une sortie sûre (raccourci pour quitter le compositeur, et rappel de garder un autre TTY ou un accès SSH).
- Ne pas installer de paquets système sans me montrer la commande d'abord.
