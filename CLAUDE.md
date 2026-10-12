# coquille — shell Wayland en Rust (nom provisoire)

Ce fichier donne le contexte du projet à Claude Code. Le garder à la racine du dépôt et le mettre à jour quand une décision change.

## Langue et conventions

- Communiquer avec moi en **français**.
- Code, identifiants, noms de crates et messages de commit en **anglais**.
- Textes visibles par l'utilisateur (barre, lanceur, notifications, menus) en **français**.
- Rust stable, `cargo fmt` et `cargo clippy -- -D warnings` propres, logs avec `tracing`.
- Pas de `unsafe` sauf nécessité documentée par un commentaire.
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

Chaque écran a une disposition parmi une liste courte :

- `Full` : une fenêtre (ou une pile d'onglets) occupe tout l'écran.
- `Columns` : deux emplacements côte à côte, ratio réglable (défaut 50/50 ; exemple réel : 58/42). **Partage par défaut sur les écrans latéraux en paysage.**
- `Rows` : deux emplacements haut / bas (moins utile en paysage, mais disponible).
- `Tabs` : tout l'écran est une pile d'onglets.
- `Free` (« écran libre ») : fenêtres flottantes qui se chevauchent, comme un bureau classique.
- Le centre 21:9 accepte aussi `TwoThirdsOneThird` hors jeu.

Changement de disposition : clic droit sur la barre de l'écran → menu.

### Règles de placement

1. **Une appli jamais rangée s'ouvre en flottant**, sur l'écran où se trouve le curseur, sans déplacer les autres fenêtres. Une petite barre propose : laisser flottante, ranger dans un emplacement, au centre, en onglet.
2. **Glisser une fenêtre** (par sa barre de titre) affiche les emplacements possibles sur les trois écrans ; relâcher la place.
3. **Un écran n'est jamais plein** : déposer sur un emplacement occupé empile la fenêtre **en onglet** avec celle qui s'y trouve.
4. **Mémoire par application** : une appli que j'ai rangée moi-même rouvre au même écran / emplacement (clé : `app_id` Wayland ou `WM_CLASS` X11). **Les fenêtres laissées flottantes ne créent aucune habitude.**
5. **Bouton ⇄ « échanger avec le centre »** sur les fenêtres des écrans latéraux : la fenêtre passe au centre, celle du centre prend sa place. Désactivé quand un jeu est ancré.
6. **Instantanés optionnels** : mémoriser la disposition actuelle des trois écrans d'un clic, la restaurer d'un clic. Rien n'oblige à préparer quoi que ce soit.
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

Sortie : `Super` libère le pointeur ; `Super + Échap` remet le jeu en fenêtre.

Précisions décidées en phase 3 (à revoir si elles gênent) :

- **Quel écran ?** Celui que l'appli nomme (xdg-shell), sinon celui où se trouve sa fenêtre. Une fenêtre X11 qui démarre en plein écran en couvrant exactement un écran va sur celui-là ; le centre est annoncé à X11 comme écran principal (RandR), ce qui y envoie les jeux Wine/Proton. Un jeu lancé avec le pointeur sur un écran latéral et qui ne dit rien s'ouvre donc là : le ranger une fois au centre suffit, la mémoire par appli fait le reste.
- **`Super` seul** (appuyé puis relâché sans autre touche ni clic) libère le pointeur du jeu qui le retient, jusqu'au prochain clic dans le jeu. Tant qu'il est libéré, les bords ne collent plus.
- **Bords collants seulement quand le jeu a le focus** : traverser l'écran d'un jeu laissé de côté (focus sur Discord) ne retient pas le curseur. Entrer sur l'écran du jeu n'est jamais retenu. Un glisser du shell n'est jamais retenu.
- **Une fenêtre qui s'ouvre pendant que le jeu a le focus ne lui prend pas le clavier** : elle apparaît sur l'écran latéral (règle 7), sans le focus, avec sa barre de rangement ; un clic dessus le lui donne.
- `Super + Échap` vise le jeu qui a le focus, sinon celui de l'écran du pointeur, sinon le seul jeu ancré.
- Le bouton ⇄ et « Au centre » disparaissent tant qu'un jeu tient le centre (ou l'écran de la fenêtre).

### Règle d'or de la souris

**Sans la touche Super, la souris ne fait jamais rien d'autre que ce que fait l'application.** Dans un jeu ancré, tous les boutons appartiennent au jeu.

- `Super + glisser` : déplacer une fenêtre (même hors barre de titre).
- `Super + glisser sur la séparation` : ajuster le ratio d'une disposition.
- **Bouton latéral maintenu + petit mouvement rapide vers un écran** : le curseur saute à cet écran, à la position où je l'avais laissé (hors jeu ancré).
- **Secouer la souris** : halo autour du curseur pour le retrouver.

## Apparence (référence : maquettes)

Les maquettes sont exportées dans `docs/maquettes/` (les ouvrir avant de toucher au rendu).

- Style proche de Windows 11 / Fluent : coins arrondis, surfaces translucides, sobre.
- Thèmes **clair** et **sombre**.
- Fenêtres : rayon **12 px**, espacement **8 px** entre emplacements et avec les bords.
- **Décorations côté serveur** (dessinées par le compositeur, via `xdg-decoration`) :
  - barre de titre **38 px** : icône, titre, sous-titre atténué ;
  - boutons à droite : ajouter un onglet, ⇄ (écrans latéraux uniquement), réduire, fermer ;
  - pas de bouton « agrandir » (la disposition s'en charge).
- Fenêtre active : **anneau d'accent de 2 px**. Accent par défaut : `#2f6fed` (clair), `#7aa7ff` (sombre).
- Aucune barre sur l'écran central : il reste entièrement libre.
- **Une barre fine (44 px) sur le bord extérieur** de chaque écran latéral, centrée verticalement, rayon 14 px :
  - gauche : lanceur, fenêtres de l'écran, heure ;
  - droite : fenêtres de l'écran, réseau, son, notifications.
- Police d'interface : Figtree (repli : police système sans-serif).

## Architecture visée

Workspace Cargo :

```
coquille/
├─ crates/
│  ├─ model/        # logique pure : écrans, dispositions, emplacements, règles, mémoire.
│  │                # AUCUNE dépendance Wayland. Entièrement testé unitairement.
│  ├─ compositor/   # binaire : Smithay, rendu, entrées, décorations, mode jeu.
│  ├─ ipc/          # protocole entre le compositeur et les clients du shell (socket Unix, JSON).
│  ├─ bar/          # client layer-shell : barres latérales.            (plus tard)
│  ├─ launcher/     # client layer-shell : lanceur.                      (plus tard)
│  └─ notifyd/      # serveur org.freedesktop.Notifications (zbus).      (plus tard)
├─ docs/maquettes/
└─ CLAUDE.md
```

- La crate `model` est le cœur : toute règle de placement doit y vivre et y être testée, le compositeur ne fait qu'appliquer ses décisions.
- Les barres, le lanceur et les notifications sont des **clients layer-shell** séparés (par exemple `smithay-client-toolkit`, ou iced avec un crate layer-shell : à évaluer).
- Configuration : `~/.config/coquille/config.toml` (identification des écrans par connecteur et/ou EDID, accent, thème, ratios, délai des bords collants, boutons de la souris).
- État persistant (mémoire par appli, instantanés) : `~/.local/state/coquille/`.

### Décisions techniques

- **Smithay est épinglé sur un commit git** (le même que niri, voir `Cargo.toml`), pas sur crates.io. Sa documentation se lit avec `cargo doc -p smithay --open` ; les exemples `anvil` et `smallvil` sont dans `~/.cargo/git/checkouts/smithay-*/`.
- **Décorations en Rust pur** : `tiny-skia` (formes) et `cosmic-text` (texte) vers des textures, shaders GLES pour l'anneau et le rognage des coins.
- **Mode imbriqué (winit) : `Alt` remplace `Super`**, que la session hôte intercepte. `scripts/ci.sh` lance `fmt`, `clippy` et les tests.
- **La donnée de la boucle d'événements est `State` lui-même** (plus de `App { display, state }`) : le gestionnaire de fenêtres X11 de Smithay l'exige. Le `Display` Wayland appartient à la source qui distribue les requêtes des clients (`main.rs`), toujours sans `unsafe`.
- **Focus clavier = `KeyFocus`** (`focus.rs`) : la surface Wayland, accompagnée de la fenêtre X11 quand c'en est une, pour que le serveur X soit prévenu des changements de focus. Le focus du pointeur reste une `WlSurface`.
- **XWayland** (`xwayland.rs`) : démarré avec le compositeur par Smithay, qui fait aussi le gestionnaire de fenêtres X11. Piège de cette version de Smithay : après `set_randr_primary_output`, il faut une autre requête X11 tout de suite, sinon **tous** les événements X11 suivants sont ignorés (voir le commentaire dans `xwayland_ready`).
- **Décisions pures du mode jeu** dans `game.rs` (testées) : tap de `Mod`, confinement, bords collants, raccourcis matériels d'un écran (`Boost`). Les règles de placement du mode jeu restent dans `model`.

### Lancer le compositeur

Deux backends, choisis tout seuls (`--backend auto`) : **imbriqué** (winit) s'il y a déjà une session graphique, **matériel** (udev) sinon.

```
cargo run -p coquille -- --three-screens --spawn kitty      # imbriqué, trois écrans simulés
cargo run -p coquille -- --role left --spawn kitty          # imbriqué, un seul écran
cargo run -p coquille -- --probe                            # examine GPU et écrans, n'affiche rien
```

- Souris : glisser une barre de titre montre les emplacements des trois écrans, relâcher range la fenêtre (en onglet si l'emplacement est pris) ; `Mod+glisser` déplace librement (la fenêtre devient flottante). Une fenêtre qui s'ouvre flottante est accompagnée d'une petite barre : « Laisser flottante », « Ranger ici » / « En onglet », « Au centre ».
- Souris, suite : tirer un bord ou un coin d'une fenêtre flottante la redimensionne (le compositeur impose alors sa taille) ; `Mod+glisser` sur la séparation de deux emplacements règle leur ratio.
- Boutons de la barre de titre : « réduire » cache la fenêtre (son onglet reste, grisé ; un clic dessus ou `Mod+R` la ramène) ; « ajouter un onglet » arme l'emplacement, et la prochaine fenêtre ouverte s'y empile, quelle que soit sa mémoire (second clic pour désarmer) ; ⇄ ; fermer.
- Raccourcis (touche Mod = `Super` sur le matériel, `Alt` en imbriqué) : `Mod+Entrée` terminal ; `Mod+1` à `6` disposition de l'écran du pointeur (plein, côte à côte, haut/bas, onglets, écran libre, deux tiers + un tiers) ; `Mod+Page suiv./préc.` onglet suivant/précédent ; `Mod+R` restaure la dernière fenêtre réduite ; `Mod+Maj+E` quitter.
- Mode jeu : `Mod+Échap` remet le jeu en fenêtre ; `Mod` seul libère le pointeur. (En imbriqué, GNOME garde `Alt+Échap` pour lui : ce raccourci ne s'essaie qu'avec `--debug-input`.)
- Applications X11 : `DISPLAY` est transmis aux programmes lancés par le compositeur (et aux services de la session avec `--export-environment`). `--no-xwayland` ou `xwayland = false` s'en passent.
- Mémoire par appli : enregistrée dans `~/.local/state/coquille/memory.json` sur le matériel ; en imbriqué, seulement avec `--state-dir`.
- Sorties de secours, sans Mod : `Ctrl+Alt+Retour arrière` quitte, `Ctrl+Alt+F1…F12` change de terminal virtuel, `--exit-after <secondes>` quitte tout seul.
- Vérifier le rendu sans regarder l'écran (imbriqué) : `--screenshot fichier.png --screenshot-delay 3000` enregistre la sortie puis quitte ; `--debug-input "2500 move 400 450; 100 click; 300 key alt+1"` rejoue des entrées (syntaxe dans `crates/compositor/src/debug.rs`) et ignore alors la vraie souris. Un glisser s'écrit `down`, `move…`, `up` ; une touche tenue, `press alt` … `release alt`. Placer le pointeur dès `0 move x y` : une fenêtre s'ouvre sur l'écran où il se trouve à ce moment-là. `rel dx dy` déplace le pointeur comme une vraie souris (chemin du backend matériel : pointeur verrouillé, bords collants, mouvement relatif), alors que `move` le pose comme le fait la fenêtre hôte.
- Clients de test utiles en imbriqué : `kitty --start-as fullscreen` (plein écran Wayland), `kitty -o linux_display_server=x11` et `glxgears` (X11), `GDK_BACKEND=x11 gnome-calculator` (menus X11). Un client SDL2 (`SDL_VIDEODRIVER=wayland` ou `x11`, `SDL_SetRelativeMouseMode`) verrouille le pointeur ; `libSDL2` est là, quelques lignes de Python `ctypes` suffisent.
- `--probe` exerce sur la vraie carte tout ce qui ne demande pas de posséder les écrans : EGL sur GBM, moteur de rendu, shaders, détection et rôles des écrans, capacité VRR de chacun. À lancer après tout changement du backend matériel.

Réglages de `config.toml` (modèle commenté : `config/config.toml` ; les réglages généraux doivent précéder les sections) : `theme`, `terminal`, `xwayland`, `[outputs]`, `[keyboard]`, et pour le mode jeu `[game]` : `sticky_edges_ms` (300 ; 0 désactive), `direct_scanout` (`true`), `vrr` (`true`).

### Installer

`scripts/install.sh` compile en release et installe **pour l'utilisateur** : `~/.local/bin/coquille`, `~/.local/bin/coquille-session` (lanceur de session), une entrée de menu (imbriqué), et `~/.config/coquille/config.toml` s'il n'existe pas (modèle : `config/config.toml`).

`scripts/install.sh --session` ajoute en plus deux sessions à l'écran de connexion (`/usr/share/wayland-sessions/`, seul endroit qui demande `sudo`) : « coquille » et « coquille (essai, 60 secondes) ». La session par défaut n'est jamais modifiée. `scripts/install.sh --uninstall` retire tout.

Journaux : `~/.local/state/coquille/session.log` (et `session.previous.log` pour le démarrage d'avant), paniques comprises. Ce que dit le serveur X va à côté, dans `xwayland.log`.

### État de la phase 2

Terminée : trois écrans réels, flottant par défaut à la taille choisie par l'appli, glisser-déposer vers les emplacements, onglets (bande dans la barre de titre), ⇄, mémoire par appli, écran libre, barre de rangement, redimensionnement des flottantes, ratio à la souris, boutons « réduire » et « ajouter un onglet ».

Reportés à la phase 4, avec les barres latérales : le menu de disposition (clic droit sur la barre) et la liste des fenêtres de l'écran, qui sera la vraie façon de retrouver une fenêtre réduite.

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
- le journal dit, pour chaque écran allumé, ce qu'il sait faire (`vrr=…`), puis `variable refresh rate switched`, et une fois par jeu `direct scanout: the buffer of the game is shown as it is` ou `direct scanout not possible for this frame: composited`. Pour les raisons détaillées du pilote, lancer depuis un TTY avec `RUST_LOG=info,smithay::backend::drm::compositor=trace` (très bavard : avec `--exit-after`).

Limites connues :

- les fenêtres X11 ne se déplacent et ne se redimensionnent que par la barre de titre et les bords du shell (les demandes `_NET_WM_MOVERESIZE` des applis qui dessinent leur propre barre, comme Steam, sont ignorées ; idem pour xdg-shell) ;
- une fenêtre X11 sans décoration qui couvre l'écran sans demander le plein écran (`_NET_WM_STATE_FULLSCREEN`) n'est pas reconnue comme un jeu ;
- pas de sélection primaire (clic du milieu) ni de glisser-déposer entre X11 et Wayland ; pas de `xwayland-keyboard-grab` ;
- pas de retour dmabuf « scanout » par surface : l'affichage direct dépend de ce que le jeu alloue de lui-même ;
- avec le VRR, un pointeur visible qui bouge fait remonter la fréquence (il n'est pas sur un plan matériel) ;
- bords collants inopérants en imbriqué avec la vraie souris : l'hôte possède le pointeur, rien ne peut le retenir ;
- notifications redirigées et « ne pas déranger » : phase 4.

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
- **Phase 4 — Shell** : barres latérales, lanceur, notifications, saut de curseur au bouton latéral, instantanés.
- **Plus tard** : calque par-dessus le jeu (performances, son par appli, capture), vignettes vivantes.

## Sécurité pendant le développement

- Tout développer et tester d'abord en **mode imbriqué (winit)**, dans ma session actuelle.
- Ne jamais remplacer ma session par défaut ni modifier le gestionnaire de connexion (GDM) sans me le demander.
- Pour les tests sur TTY : prévoir une sortie sûre (raccourci pour quitter le compositeur, et rappel de garder un autre TTY ou un accès SSH).
- Ne pas installer de paquets système sans me montrer la commande d'abord.
