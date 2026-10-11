#!/usr/bin/env bash
# Installs coquille for the current user: no root, nothing outside the home
# directory, and no login session is touched.
#
#   scripts/install.sh               build and install
#   scripts/install.sh --uninstall   remove what was installed
#
# PREFIX (default ~/.local) chooses where; TERMINAL chooses the terminal the
# menu entries open inside the compositor.
set -euo pipefail
cd "$(dirname "$0")/.."

prefix="${PREFIX:-$HOME/.local}"
bin="$prefix/bin/coquille"
entry="$prefix/share/applications/coquille.desktop"
# Logs of sessions started from the menu, to look at after a problem.
log_dir="${XDG_STATE_HOME:-$HOME/.local/state}/coquille"

if [[ "${1:-}" == "--uninstall" ]]; then
    rm -f -- "${bin:?}" "${entry:?}"
    echo "coquille est désinstallé de $prefix."
    echo "Les journaux restent dans $log_dir ; supprimez ce dossier si vous n'en voulez plus."
    exit 0
fi

terminal="${TERMINAL:-}"
if [[ -z "$terminal" ]]; then
    for candidate in kitty alacritty foot; do
        if command -v "$candidate" >/dev/null; then
            terminal="$candidate"
            break
        fi
    done
fi
if [[ -z "$terminal" ]]; then
    echo "Aucun terminal trouvé : relancez avec TERMINAL=<commande>." >&2
    exit 1
fi

cargo build --release --locked -p coquille
install -Dm755 target/release/coquille "$bin"

# Started from the menu there is no terminal to read: the log goes to a file.
launch() {
    printf '"%s" --log-file "%s/coquille.log" --spawn "%s" %s' "$bin" "$log_dir" "$terminal" "$1"
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
Actions=left;center;right;

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

if command -v update-desktop-database >/dev/null; then
    update-desktop-database -q "$(dirname "$entry")" || true
fi

cat <<DONE

coquille est installé :
  programme   $bin
  menu        $entry
  journal     $log_dir/coquille.log  (sessions lancées depuis le menu)
  terminal    $terminal

Lancer depuis un terminal :
  coquille --spawn $terminal
  coquille --fullscreen --role left --spawn $terminal

Quitter : Alt+Maj+E.        Désinstaller : scripts/install.sh --uninstall
DONE
