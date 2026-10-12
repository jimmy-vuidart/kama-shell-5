#!/usr/bin/env bash
# Builds coquille and its dock, then runs them nested, in a window of the
# current session: the quick way to try a change.
#
#   scripts/debug.sh                 the three screens, side by side
#   scripts/debug.sh left            one screen, as the left one (the dock's)
#   scripts/debug.sh center -f       one screen, as the center, full screen
#   scripts/debug.sh --mock          the dock on made-up windows and services
#   scripts/debug.sh -- --exit-after 20     what follows -- goes to coquille
#
# The dock talks to the real services, but its session buttons only pretend:
# a trial in a window must not restart the machine.
set -euo pipefail
cd "$(dirname "$0")/.."

usage() {
    cat <<'HELP'
Usage : scripts/debug.sh [ÉCRAN] [OPTIONS] [-- OPTIONS DE COQUILLE]

Compile coquille et le dock (debug), puis les lance en imbriqué.

ÉCRAN
  three            les trois écrans simulés côte à côte (défaut)
  left|center|right  un seul écran, avec ce rôle (le dock vit à gauche)

OPTIONS
  -f, --fullscreen   occupe tout l'écran où la fenêtre s'ouvre
      --mock         dock entièrement simulé (fenêtres, son, réseau…)
      --no-dock      sans le dock
      --release      compile en release (plus long, plus fluide)
  -h, --help         cette aide

Dans la fenêtre, Alt remplace Super : Alt seul ouvre le tiroir,
Alt+Maj+E quitte. RUST_LOG règle le journal.
HELP
}

screen=three
profile=debug
dock=real
fullscreen=()
extra=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        three | left | center | right) screen="$1" ;;
        -f | --fullscreen) fullscreen=(--fullscreen) ;;
        --mock) dock=mock ;;
        --no-dock) dock=none ;;
        --release) profile=release ;;
        -h | --help)
            usage
            exit 0
            ;;
        --)
            shift
            extra=("$@")
            break
            ;;
        *)
            echo "Option inconnue : $1" >&2
            usage >&2
            exit 2
            ;;
    esac
    shift
done

if [[ "$screen" == three ]]; then
    layout=(--three-screens)
else
    layout=(--role "$screen")
fi

build=(cargo build -p coquille -p coquille-dock)
if [[ "$profile" == release ]]; then
    build+=(--release)
fi
"${build[@]}"
bin="$PWD/target/$profile"

spawn=()
case "$dock" in
    # Everything real but the session: see the top of this file.
    real) spawn+=(--spawn "$bin/coquille-dock --backends compositor,audio,network,bluetooth,brightness") ;;
    mock) spawn+=(--spawn "$bin/coquille-dock --mock") ;;
    none) ;;
esac
for terminal in kitty alacritty foot; do
    if command -v "$terminal" >/dev/null; then
        spawn+=(--spawn "$terminal")
        break
    fi
done

# Smithay is talkative at `info`: only coquille and its dock say everything.
export RUST_LOG="${RUST_LOG:-warn,wgpu_hal=error,coquille=debug,coquille_dock=debug}"
exec "$bin/coquille" --backend nested "${layout[@]}" "${fullscreen[@]}" "${spawn[@]}" "${extra[@]}"
