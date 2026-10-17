# 25 — Installer coquille comme paquet AUR

- **Priorité :** faible (pour l'instant, `scripts/install.sh` suffit)
- **Nature :** évolution
- **Zone :** nouveau `packaging/` (ou `PKGBUILD` à la racine), `scripts/install.sh`, `scripts/coquille-session`, `crates/compositor/src/wallpaper.rs`, `config/config.toml`
- **Se vérifie :** `makepkg` dans un chroot propre (`extra-x86_64-build`), puis une session ouverte depuis l'écran de connexion
- **Certitude :** décision de Jimmy (17 octobre 2026) ; rien n'est écrit

## Constat

coquille s'installe par `scripts/install.sh`, pour l'utilisateur seul
(`~/.local`), plus deux fichiers de session dans `/usr/share/wayland-sessions`
avec `--session`. Ce qu'il lui faut sur le système n'est déclaré nulle part :
le script vérifie seulement la présence du thème d'icônes Reversal et donne
la commande pour l'installer (`paru -S reversal-icon-theme-git`).

À terme, le projet doit s'installer comme un paquet AUR qui déclare ses
dépendances, à commencer par celle des icônes.

## Piste

Un `PKGBUILD` `coquille-git` (paquet `-git` tant qu'il n'y a pas de version
publiée) :

- `depends` : `reversal-icon-theme-git` (icônes du dock et des barres de
  titre), `xorg-xwayland`, `libinput`, `seatd` ou `systemd-libs` (libseat),
  `libxkbcommon`, `mesa` (EGL, GBM), `wayland`, `fontconfig` ; à vérifier
  avec `namcap` et `ldd` sur les binaires ;
- `makedepends` : `cargo` (ou `rustup`), `git` ;
- `optdepends` : `pipewire-pulse` (mixeur : `pactl`, `parec`),
  `networkmanager` (`nmcli`), `bluez-utils` (`bluetoothctl`), `ddcutil`
  (luminosité), un terminal (`kitty`, `alacritty`, `foot`), `ttf-figtree`
  si le paquet existe ;
- `build()` : `cargo build --release --locked --frozen`, après
  `cargo fetch --locked` dans `prepare()` ; Smithay est épinglé sur un commit
  git, `cargo fetch` le récupère ;
- `package()` : binaires dans `/usr/bin`, `coquille-session` et
  `coquille-dock-watch` aussi, fonds d'écran dans
  `/usr/share/coquille/wallpapers` (déjà cherchés là par `wallpaper.rs`),
  modèle de configuration dans `/usr/share/coquille/config.toml`, entrées
  `coquille-settings.desktop` et de session dans `/usr/share/applications` et
  `/usr/share/wayland-sessions`, licence.

Ce qu'il faut changer dans le dépôt avant :

1. `coquille-session` reçoit le terminal par `sed` à l'installation
   (`@TERMINAL@`) : le choisir au lancement (`$TERMINAL`, `terminal` de la
   configuration, puis kitty, alacritty, foot).
2. La configuration n'est créée que par `install.sh` : la créer au premier
   lancement depuis `/usr/share/coquille/config.toml` quand
   `~/.config/coquille/config.toml` manque.
3. L'entrée de menu imbriquée de `install.sh` (`coquille.desktop`) : la
   garder dans le paquet ou non (elle sert au développement).
4. `install.sh` reste pour le développement ; le dire dans `CLAUDE.md`.

## Terminé quand

- `makepkg -si` dans un chroot propre construit et installe coquille, et
  `namcap` ne signale aucune dépendance manquante.
- Installer `coquille-git` installe aussi `reversal-icon-theme-git`.
- La session « coquille » de l'écran de connexion démarre depuis le paquet,
  sans `~/.local/bin`, et le dock affiche les icônes Reversal.

## Liens

- Voisin de : 08 (le thème d'icônes n'est plus demandé à `gsettings` quand
  Reversal est installé).
