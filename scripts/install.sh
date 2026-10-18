#!/usr/bin/env bash
# Installs coquille for the current user.
#
#   scripts/install.sh               build and install in the home directory
#   scripts/install.sh --session     same, and add coquille to the login screen
#   scripts/install.sh --uninstall   remove everything that was installed
#
# The icons come from the AUR package reversal-icon-theme-git: the script
# says when it is missing, and never installs it.
#
# Only --session needs root, for the two files of the login screen and the
# portal file of the keyring; it never changes the default session. PREFIX
# (default ~/.local) chooses where the rest goes.
set -euo pipefail
cd "$(dirname "$0")/.."

prefix="${PREFIX:-$HOME/.local}"
bin="$prefix/bin/coquille"
dock_bin="$prefix/bin/coquille-dock"
dock_watch="$prefix/bin/coquille-dock-watch"
settings_bin="$prefix/bin/coquille-settings"
keyring_bin="$prefix/bin/coquille-keyring"
session_bin="$prefix/bin/coquille-session"
entry="$prefix/share/applications/coquille.desktop"
# Named after the windows of the settings screen: their title bar and their
# icon in the dock take its name and its icon.
settings_entry="$prefix/share/applications/coquille-settings.desktop"
wallpapers_dir="$prefix/share/coquille/wallpapers"
config="${XDG_CONFIG_HOME:-$HOME/.config}/coquille/config.toml"
# Logs of sessions started from the menu or the login screen.
log_dir="${XDG_STATE_HOME:-$HOME/.local/state}/coquille"
# Where display managers look for Wayland sessions.
sessions="${SESSIONS_DIR:-/usr/share/wayland-sessions}"
session_entries=("$sessions/coquille.desktop" "$sessions/coquille-essai.desktop")
# Where xdg-desktop-portal looks for its backends (system-wide only), and for
# the choices of the user among them.
portals="${PORTALS_DIR:-/usr/share/xdg-desktop-portal/portals}"
portal_entry="$portals/coquille.portal"
portals_config="${XDG_CONFIG_HOME:-$HOME/.config}/xdg-desktop-portal/coquille-portals.conf"

# Runs a command as root unless the directory it writes in is already
# writable (a scratch SESSIONS_DIR or PORTALS_DIR, for trying the script out).
as_root() {
    local target="$1"
    shift
    if [[ -w "$target" || ( ! -e "$target" && -w "$(dirname "$target")" ) ]]; then
        "$@"
    else
        sudo "$@"
    fi
}

if [[ "${1:-}" == "--uninstall" ]]; then
    rm -f -- "${bin:?}" "${dock_bin:?}" "${dock_watch:?}" "${settings_bin:?}" "${session_bin:?}" \
        "${keyring_bin:?}" "${entry:?}" "${settings_entry:?}" "${portals_config:?}"
    rm -rf -- "${wallpapers_dir:?}"
    for session_entry in "${session_entries[@]}"; do
        if [[ -e "$session_entry" ]]; then
            as_root "$sessions" rm -f -- "${session_entry:?}"
        fi
    done
    if [[ -e "$portal_entry" ]]; then
        as_root "$portals" rm -f -- "${portal_entry:?}"
    fi
    echo "coquille est désinstallé."
    echo "Restent, si vous voulez les supprimer : $config et $log_dir"
    echo "(le trousseau y est : keyring.bin, et sa clé scellée keyring.key)"
    exit 0
fi

cargo build --release --locked -p coquille -p coquille-dock -p coquille-settings \
    -p coquille-keyring
install -Dm755 target/release/coquille "$bin"
install -Dm755 target/release/coquille-dock "$dock_bin"
install -Dm755 target/release/coquille-settings "$settings_bin"
install -Dm755 target/release/coquille-keyring "$keyring_bin"
install -Dm755 scripts/coquille-dock-watch "$dock_watch"
install -Dm755 scripts/coquille-session "$session_bin"
mkdir -p "$wallpapers_dir"
if compgen -G "assets/wallpapers/*.png" > /dev/null; then
    install -Dm644 -t "$wallpapers_dir" assets/wallpapers/*.png
fi

if [[ ! -e "$config" ]]; then
    install -Dm644 config/config.toml "$config"
    config_note="créée (rôles des écrans : à vérifier)"
else
    config_note="conservée"
fi

# Sandboxed applications reach the keyring through this choice of portal;
# one the user wrote is theirs.
if [[ ! -e "$portals_config" ]]; then
    install -Dm644 config/coquille-portals.conf "$portals_config"
fi

# The icons of the shell come from a package, never installed from here.
icons_package=reversal-icon-theme-git
if command -v pacman >/dev/null && pacman -Q "$icons_package" >/dev/null 2>&1; then
    icons_note="paquet $icons_package"
elif compgen -G "/usr/share/icons/Reversal*/index.theme" >/dev/null \
    || compgen -G "${XDG_DATA_HOME:-$HOME/.local/share}/icons/Reversal*/index.theme" >/dev/null \
    || compgen -G "$HOME/.icons/Reversal*/index.theme" >/dev/null; then
    icons_note="Reversal trouvé, hors du paquet $icons_package"
else
    icons_note="ABSENTES : paru -S $icons_package (AUR), sinon le thème d'icônes du système"
fi

# Started from the menu there is no terminal to read: the log goes to a file.
# The dock comes along, as in the real session.
launch() {
    printf '"%s" --backend nested --log-file "%s/coquille.log" --spawn "exec %s" %s' \
        "$bin" "$log_dir" "$dock_watch" "$1"
}

mkdir -p "$(dirname "$entry")"
cat >"$entry" <<DESKTOP
[Desktop Entry]
Type=Application
Name=coquille
GenericName=Compositeur Wayland
Comment=Compositeur « Centre + compagnons », imbriqué dans la session actuelle
Exec=$(launch "--role right")
Icon=preferences-desktop-display
Terminal=false
Categories=System;
Actions=three;left;center;right;

[Desktop Action three]
Name=Plein écran, les trois écrans simulés
Exec=$(launch "--fullscreen --three-screens")

[Desktop Action left]
Name=Plein écran, comme écran gauche
Exec=$(launch "--fullscreen --role left")

[Desktop Action center]
Name=Plein écran, comme écran central
Exec=$(launch "--fullscreen --role center")

[Desktop Action right]
Name=Plein écran, comme écran droit
Exec=$(launch "--fullscreen --role right")
DESKTOP

# The settings screen only makes sense inside coquille, where the dock opens
# it; elsewhere it still edits the configuration file.
cat >"$settings_entry" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Paramètres de coquille
GenericName=Paramètres
Comment=Apparence, écrans et dock, mode jeu, clavier
Exec="$settings_bin"
Icon=preferences-system
Terminal=false
Categories=Settings;
StartupWMClass=coquille-settings
DESKTOP

if command -v update-desktop-database >/dev/null; then
    update-desktop-database -q "$(dirname "$entry")" || true
fi

# The entries of the login screen name the session script, which was just
# replaced: once there, they need no second installation.
if [[ -e "${session_entries[0]}" ]]; then
    session_note="déjà installée dans $sessions"
else
    session_note="non installée (scripts/install.sh --session)"
fi
if [[ "${1:-}" == "--session" ]]; then
    # The display managers derived from SDDM (plasmalogin among them) hand
    # Exec to a shell script that runs it unquoted (`exec $@`): quotes would
    # stay part of the program name, and a space would cut it in two. The
    # path is therefore written bare, and must have no blank in it.
    if [[ "$session_bin" =~ [[:space:]] ]]; then
        echo "Le chemin $session_bin contient un espace : l'écran de connexion ne saurait pas le lancer." >&2
        echo "Choisissez un PREFIX sans espace." >&2
        exit 1
    fi
    staging="$(mktemp -d)"
    cat >"$staging/coquille.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=coquille
Comment=Compositeur « Centre + compagnons »
Exec=$session_bin
DesktopNames=coquille
DESKTOP
    cat >"$staging/coquille-essai.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=coquille (essai, 60 secondes)
Comment=Se ferme tout seul après une minute : pour un premier essai
Exec=$session_bin --exit-after 60
DesktopNames=coquille
DESKTOP
    echo "Ajout de coquille à l'écran de connexion (droits administrateur) :"
    as_root "$sessions" install -Dm644 -t "$sessions" "$staging/coquille.desktop" "$staging/coquille-essai.desktop"
    session_note="installée dans $sessions"
    as_root "$portals" install -Dm644 -t "$portals" config/coquille.portal
fi
if [[ -e "$portal_entry" ]]; then
    portal_note="portail $portal_entry"
else
    portal_note="portail de gnome-keyring en attendant (scripts/install.sh --session)"
fi

cat <<DONE

coquille est installé :
  programme        $bin
  dock             $dock_bin
  paramètres       $settings_bin  (bouton du dock, à côté de la déconnexion)
  session          $session_bin  (lance aussi le dock et le trousseau)
  trousseau        $keyring_bin  ($portal_note)
  menu             $entry
  configuration    $config  ($config_note)
  fonds d'écran    $wallpapers_dir/
  icônes           $icons_note
  écran de connexion  $session_note
  journaux         $log_dir/

Dans la session actuelle (fenêtre) :   coquille --three-screens
Sur les vrais écrans, depuis un TTY :  coquille-session --exit-after 60
Quitter : Super+Maj+E (Alt+Maj+E en fenêtre) ou Ctrl+Alt+Retour arrière.
Sans le dock : COQUILLE_NO_DOCK=1 coquille-session
Désinstaller : scripts/install.sh --uninstall
DONE
