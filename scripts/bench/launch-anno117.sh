#!/usr/bin/env bash
# Steam launch wrapper for Anno 117 benchmarking (Coquille vs GNOME).
#
# Steam Launch Options:
#   /home/jimmy/Projets/kama-shell-5/scripts/bench/launch-anno117.sh %command%
#
# Environment overrides:
#   BENCH_ROOT   Base directory for results (default: ~/benchmarks/anno117)
#   BENCH_LABEL  Override detected session name (e.g. coquille-no-scanout)
set -euo pipefail

if [[ $# -eq 0 ]]; then
    echo "Usage: $0 <game command...>" >&2
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE_CONF="$SCRIPT_DIR/mangohud-anno117.conf"

detect_session() {
    if [[ -n "${BENCH_LABEL:-}" ]]; then
        echo "$BENCH_LABEL"
        return
    fi
    local desktop="${XDG_CURRENT_DESKTOP:-${XDG_SESSION_DESKTOP:-unknown}}"
    case "${desktop,,}" in
        *coquille*) echo "coquille" ;;
        *gnome*)    echo "gnome" ;;
        *)
            if pgrep -x coquille >/dev/null 2>&1; then
                echo "coquille"
            elif pgrep -x gnome-shell >/dev/null 2>&1; then
                echo "gnome"
            else
                echo "${desktop,,}"
            fi
            ;;
    esac
}

SESSION="$(detect_session)"
TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
BENCH_ROOT="${BENCH_ROOT:-$HOME/benchmarks/anno117}"
RUN_DIR="$BENCH_ROOT/$SESSION/$TIMESTAMP"
mkdir -p "$RUN_DIR"

# Build per-run MangoHud config with the dedicated output folder
RUN_CONF="$RUN_DIR/mangohud.conf"
cp "$BASE_CONF" "$RUN_CONF"
printf '\noutput_folder=%s\n' "$RUN_DIR" >> "$RUN_CONF"

# Save environment metadata for reproducibility checks
{
    echo "session=$SESSION"
    echo "timestamp=$TIMESTAMP"
    echo "xdg_current_desktop=${XDG_CURRENT_DESKTOP:-}"
    echo "xdg_session_type=${XDG_SESSION_TYPE:-}"
    echo "kernel=$(uname -r)"
    echo "cpu_governor=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo unknown)"
    echo "command=$*"
} > "$RUN_DIR/env.txt"

COQUILLE_LOG="${XDG_STATE_HOME:-$HOME/.local/state}/coquille/session.log"
COQUILLE_LOG_START_LINE=0
if [[ -f "$COQUILLE_LOG" ]]; then
    COQUILLE_LOG_START_LINE="$(wc -l < "$COQUILLE_LOG")"
fi

MONITOR_PID=""
cleanup() {
    if [[ -n "$MONITOR_PID" ]] && kill -0 "$MONITOR_PID" 2>/dev/null; then
        kill "$MONITOR_PID" 2>/dev/null || true
        wait "$MONITOR_PID" 2>/dev/null || true
    fi

    # Extract Coquille game-mode events (direct scanout / VRR / anchored) that occurred during this run
    if [[ -f "$COQUILLE_LOG" ]]; then
        tail -n +"$((COQUILLE_LOG_START_LINE + 1))" "$COQUILLE_LOG" \
            | grep -E "direct scanout|variable refresh rate|anchored|boost" \
            > "$RUN_DIR/coquille-game-events.log" || true
    fi
}
trap cleanup EXIT

"$SCRIPT_DIR/monitor-session.sh" "$RUN_DIR" 1 &
MONITOR_PID=$!

export MANGOHUD=1
export MANGOHUD_CONFIGFILE="$RUN_CONF"

"$@"
