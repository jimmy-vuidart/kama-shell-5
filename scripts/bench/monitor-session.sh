#!/usr/bin/env bash
# Samples GPU (nvidia-smi) and compositor/Xwayland CPU & RSS memory every second
# into system-metrics.csv inside the given output directory.
set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "Usage: $0 <output_dir> [interval_sec]" >&2
    exit 1
fi

OUT_DIR="$1"
INTERVAL="${2:-1}"
OUT_CSV="$OUT_DIR/system-metrics.csv"

mkdir -p "$OUT_DIR"

echo "timestamp,gpu_util_pct,gpu_clock_mhz,mem_clock_mhz,vram_used_mib,gpu_temp_c,gpu_power_w,pstate,compositor_name,compositor_cpu_pct,compositor_rss_mib,xwayland_cpu_pct,xwayland_rss_mib,dock_cpu_pct,dock_rss_mib" > "$OUT_CSV"

sample_proc() {
    local pattern="$1"
    # Sum CPU% and RSS (KiB -> MiB) across matching processes; emit "0,0" if absent.
    ps -eo comm,pcpu,rss --no-headers | awk -v pat="$pattern" '
        $1 == pat { cpu += $2; rss += $3; found = 1 }
        END {
            if (found) printf "%.2f,%.1f", cpu, rss / 1024.0;
            else printf "0.00,0.0";
        }
    '
}

while true; do
    ts="$(date +%Y-%m-%dT%H:%M:%S)"

    if command -v nvidia-smi >/dev/null 2>&1; then
        gpu_line="$(nvidia-smi --query-gpu=utilization.gpu,clocks.gr,clocks.mem,memory.used,temperature.gpu,power.draw,pstate \
            --format=csv,noheader,nounits 2>/dev/null | head -n1 | tr -d ' ')"
    else
        gpu_line="0,0,0,0,0,0,N/A"
    fi

    if pgrep -x coquille >/dev/null 2>&1; then
        comp_name="coquille"
        comp_stats="$(sample_proc coquille)"
    elif pgrep -x gnome-shell >/dev/null 2>&1; then
        comp_name="gnome-shell"
        comp_stats="$(sample_proc gnome-shell)"
    else
        comp_name="unknown"
        comp_stats="0.00,0.0"
    fi

    xw_stats="$(sample_proc Xwayland)"
    dock_stats="$(sample_proc coquille-dock)"

    echo "${ts},${gpu_line},${comp_name},${comp_stats},${xw_stats},${dock_stats}" >> "$OUT_CSV"
    sleep "$INTERVAL"
done
