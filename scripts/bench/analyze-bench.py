#!/usr/bin/env python3
"""Analyze and compare MangoHud + system-metrics benchmark runs (GNOME vs Coquille)."""

from __future__ import annotations

import argparse
import csv
import json
import math
import statistics
import sys
from dataclasses import dataclass, asdict
from pathlib import Path


@dataclass
class RunMetrics:
    name: str
    frames: int
    duration_s: float
    fps_avg: float
    fps_median: float
    fps_1_low: float
    fps_01_low: float
    ft_avg_ms: float
    ft_median_ms: float
    ft_p95_ms: float
    ft_p99_ms: float
    ft_p999_ms: float
    ft_stddev_ms: float
    stutter_2x_pct: float
    stutter_2x_count: int
    over_16ms_pct: float
    over_33ms_pct: float
    compositor_name: str | None = None
    compositor_cpu_avg: float | None = None
    compositor_cpu_max: float | None = None
    compositor_rss_mib: float | None = None
    xwayland_cpu_avg: float | None = None
    gpu_util_avg: float | None = None
    gpu_clock_avg_mhz: float | None = None
    vram_avg_mib: float | None = None
    gpu_power_avg_w: float | None = None
    scanout_active: int = 0
    scanout_failed: int = 0
    vrr_switches: int = 0


def percentile(sorted_vals: list[float], pct: float) -> float:
    """Linear-interpolated percentile on an already sorted list (pct in 0..100)."""
    if not sorted_vals:
        return 0.0
    if len(sorted_vals) == 1:
        return sorted_vals[0]
    k = (len(sorted_vals) - 1) * (pct / 100.0)
    f = math.floor(k)
    c = math.ceil(k)
    if f == c:
        return sorted_vals[int(k)]
    return sorted_vals[f] * (c - k) + sorted_vals[c] * (k - f)


def parse_mangohud_csv(csv_path: Path) -> list[float]:
    """Return per-frame frametimes in milliseconds from a MangoHud CSV log."""
    frametimes: list[float] = []
    with csv_path.open("r", encoding="utf-8", errors="replace") as fh:
        lines = fh.readlines()

    header_idx = None
    for i, line in enumerate(lines):
        cols = [c.strip().lower() for c in line.split(",")]
        if "frametime" in cols or "fps" in cols:
            # Skip the 1st system-info header line if it happens not to contain frametime/fps
            header_idx = i
            break

    if header_idx is None:
        return frametimes

    reader = csv.DictReader(lines[header_idx:])
    # Normalize fieldnames
    if not reader.fieldnames:
        return frametimes
    norm_fields = {f: f.strip().lower() for f in reader.fieldnames}

    ft_col = next((orig for orig, low in norm_fields.items() if low == "frametime"), None)
    fps_col = next((orig for orig, low in norm_fields.items() if low == "fps"), None)

    for row in reader:
        val_ft = None
        if ft_col and row.get(ft_col):
            try:
                v = float(row[ft_col])
                if v > 0:
                    val_ft = v
            except ValueError:
                pass
        if val_ft is None and fps_col and row.get(fps_col):
            try:
                fps_v = float(row[fps_col])
                if fps_v > 0:
                    val_ft = 1000.0 / fps_v
            except ValueError:
                pass
        if val_ft is not None:
            frametimes.append(val_ft)

    return frametimes


def parse_system_metrics(sys_csv: Path) -> dict[str, float | str]:
    if not sys_csv.is_file():
        return {}
    comp_name = None
    comp_cpus: list[float] = []
    comp_rss: list[float] = []
    xw_cpus: list[float] = []
    gpu_utils: list[float] = []
    gpu_clocks: list[float] = []
    vram_used: list[float] = []
    gpu_power: list[float] = []

    with sys_csv.open("r", encoding="utf-8", errors="replace") as fh:
        reader = csv.DictReader(fh)
        for row in reader:
            try:
                if row.get("compositor_name"):
                    comp_name = row["compositor_name"]
                comp_cpus.append(float(row["compositor_cpu_pct"]))
                comp_rss.append(float(row["compositor_rss_mib"]))
                xw_cpus.append(float(row["xwayland_cpu_pct"]))
                gpu_utils.append(float(row["gpu_util_pct"]))
                gpu_clocks.append(float(row["gpu_clock_mhz"]))
                vram_used.append(float(row["vram_used_mib"]))
                gpu_power.append(float(row["gpu_power_w"]))
            except (KeyError, ValueError):
                continue

    if not comp_cpus:
        return {}
    return {
        "compositor_name": comp_name or "unknown",
        "compositor_cpu_avg": statistics.mean(comp_cpus),
        "compositor_cpu_max": max(comp_cpus),
        "compositor_rss_mib": statistics.mean(comp_rss),
        "xwayland_cpu_avg": statistics.mean(xw_cpus),
        "gpu_util_avg": statistics.mean(gpu_utils),
        "gpu_clock_avg_mhz": statistics.mean(gpu_clocks),
        "vram_avg_mib": statistics.mean(vram_used),
        "gpu_power_avg_w": statistics.mean(gpu_power),
    }


def parse_coquille_events(log_path: Path) -> tuple[int, int, int]:
    if not log_path.is_file():
        return (0, 0, 0)
    active = 0
    failed = 0
    vrr = 0
    with log_path.open("r", encoding="utf-8", errors="replace") as fh:
        for line in fh:
            low = line.lower()
            if "direct scanout: the buffer of the game is shown as it is" in low:
                active += 1
            elif "direct scanout failed" in low:
                failed += 1
            elif "variable refresh rate switched" in low:
                vrr += 1
    return (active, failed, vrr)


def compute_run_metrics(csv_path: Path) -> RunMetrics | None:
    frametimes = parse_mangohud_csv(csv_path)
    if len(frametimes) < 10:
        return None

    total_ms = sum(frametimes)
    duration_s = total_ms / 1000.0
    sorted_ft = sorted(frametimes)
    ft_avg = total_ms / len(frametimes)
    ft_med = statistics.median(sorted_ft)
    ft_p95 = percentile(sorted_ft, 95.0)
    ft_p99 = percentile(sorted_ft, 99.0)
    ft_p999 = percentile(sorted_ft, 99.9)
    ft_std = statistics.stdev(frametimes)

    # 1% low and 0.1% low computed from worst frametimes (standard Frametime-based definition)
    fps_avg = 1000.0 / ft_avg
    fps_median = 1000.0 / ft_med
    fps_1_low = 1000.0 / ft_p99 if ft_p99 > 0 else 0.0
    fps_01_low = 1000.0 / ft_p999 if ft_p999 > 0 else 0.0

    stutter_thresh = ft_med * 2.0
    stutter_2x_count = sum(1 for ft in frametimes if ft > stutter_thresh)
    stutter_2x_pct = 100.0 * stutter_2x_count / len(frametimes)
    over_16ms_pct = 100.0 * sum(1 for ft in frametimes if ft > 16.67) / len(frametimes)
    over_33ms_pct = 100.0 * sum(1 for ft in frametimes if ft > 33.33) / len(frametimes)

    sys_info = parse_system_metrics(csv_path.parent / "system-metrics.csv")
    scan_ok, scan_fail, vrr_sw = parse_coquille_events(csv_path.parent / "coquille-game-events.log")

    return RunMetrics(
        name=f"{csv_path.parent.name}/{csv_path.name}",
        frames=len(frametimes),
        duration_s=duration_s,
        fps_avg=fps_avg,
        fps_median=fps_median,
        fps_1_low=fps_1_low,
        fps_01_low=fps_01_low,
        ft_avg_ms=ft_avg,
        ft_median_ms=ft_med,
        ft_p95_ms=ft_p95,
        ft_p99_ms=ft_p99,
        ft_p999_ms=ft_p999,
        ft_stddev_ms=ft_std,
        stutter_2x_pct=stutter_2x_pct,
        stutter_2x_count=stutter_2x_count,
        over_16ms_pct=over_16ms_pct,
        over_33ms_pct=over_33ms_pct,
        compositor_name=sys_info.get("compositor_name"),  # type: ignore[arg-type]
        compositor_cpu_avg=sys_info.get("compositor_cpu_avg"),  # type: ignore[arg-type]
        compositor_cpu_max=sys_info.get("compositor_cpu_max"),  # type: ignore[arg-type]
        compositor_rss_mib=sys_info.get("compositor_rss_mib"),  # type: ignore[arg-type]
        xwayland_cpu_avg=sys_info.get("xwayland_cpu_avg"),  # type: ignore[arg-type]
        gpu_util_avg=sys_info.get("gpu_util_avg"),  # type: ignore[arg-type]
        gpu_clock_avg_mhz=sys_info.get("gpu_clock_avg_mhz"),  # type: ignore[arg-type]
        vram_avg_mib=sys_info.get("vram_avg_mib"),  # type: ignore[arg-type]
        gpu_power_avg_w=sys_info.get("gpu_power_avg_w"),  # type: ignore[arg-type]
        scanout_active=scan_ok,
        scanout_failed=scan_fail,
        vrr_switches=vrr_sw,
    )


def find_mangohud_csvs(root: Path) -> list[Path]:
    if root.is_file():
        return [root]
    csvs = [
        p for p in sorted(root.rglob("*.csv"))
        if p.name != "system-metrics.csv" and not p.name.endswith("_summary.csv")
    ]
    return csvs


def aggregate_runs(label: str, root: Path) -> tuple[list[RunMetrics], dict[str, float]]:
    runs: list[RunMetrics] = []
    for csv_path in find_mangohud_csvs(root):
        m = compute_run_metrics(csv_path)
        if m is not None:
            runs.append(m)

    if not runs:
        return ([], {})

    def avg_attr(attr: str) -> float:
        vals = [getattr(r, attr) for r in runs if getattr(r, attr) is not None]
        return statistics.mean(vals) if vals else 0.0

    summary = {
        "runs": float(len(runs)),
        "frames": float(sum(r.frames for r in runs)),
        "fps_avg": avg_attr("fps_avg"),
        "fps_median": avg_attr("fps_median"),
        "fps_1_low": avg_attr("fps_1_low"),
        "fps_01_low": avg_attr("fps_01_low"),
        "ft_median_ms": avg_attr("ft_median_ms"),
        "ft_p95_ms": avg_attr("ft_p95_ms"),
        "ft_p99_ms": avg_attr("ft_p99_ms"),
        "ft_p999_ms": avg_attr("ft_p999_ms"),
        "ft_stddev_ms": avg_attr("ft_stddev_ms"),
        "stutter_2x_pct": avg_attr("stutter_2x_pct"),
        "compositor_cpu_avg": avg_attr("compositor_cpu_avg"),
        "compositor_rss_mib": avg_attr("compositor_rss_mib"),
        "xwayland_cpu_avg": avg_attr("xwayland_cpu_avg"),
        "gpu_util_avg": avg_attr("gpu_util_avg"),
        "gpu_clock_avg_mhz": avg_attr("gpu_clock_avg_mhz"),
        "vram_avg_mib": avg_attr("vram_avg_mib"),
        "gpu_power_avg_w": avg_attr("gpu_power_avg_w"),
        "scanout_active": float(sum(r.scanout_active for r in runs)),
        "scanout_failed": float(sum(r.scanout_failed for r in runs)),
        "vrr_switches": float(sum(r.vrr_switches for r in runs)),
    }
    return (runs, summary)


def format_delta(base: float, target: float, higher_is_better: bool) -> str:
    if base == 0:
        return "N/A"
    pct = ((target - base) / base) * 100.0
    sign = "+" if pct >= 0 else ""
    good = (pct >= 0) if higher_is_better else (pct <= 0)
    tag = "OK" if abs(pct) < 1.0 else ("+" if good else "-")
    return f"{sign}{pct:.2f}% [{tag}]"


def render_markdown(
    base_label: str,
    base_sum: dict[str, float],
    tgt_label: str,
    tgt_sum: dict[str, float],
) -> str:
    rows = [
        ("Runs", "runs", "{:.0f}", None),
        ("FPS Moyen", "fps_avg", "{:.1f}", True),
        ("FPS Médian", "fps_median", "{:.1f}", True),
        ("1% Low (FPS)", "fps_1_low", "{:.1f}", True),
        ("0.1% Low (FPS)", "fps_01_low", "{:.1f}", True),
        ("Frametime Médian (ms)", "ft_median_ms", "{:.2f}", False),
        ("Frametime P99 (ms)", "ft_p99_ms", "{:.2f}", False),
        ("Frametime P99.9 (ms)", "ft_p999_ms", "{:.2f}", False),
        ("Écart-type FT (ms)", "ft_stddev_ms", "{:.2f}", False),
        ("Stutter >2x médiane (%)", "stutter_2x_pct", "{:.3f}", False),
        ("CPU Compositeur (%)", "compositor_cpu_avg", "{:.2f}", False),
        ("RAM Compositeur (MiB)", "compositor_rss_mib", "{:.1f}", False),
        ("CPU Xwayland (%)", "xwayland_cpu_avg", "{:.2f}", False),
        ("GPU Util (%)", "gpu_util_avg", "{:.1f}", None),
        ("GPU Clock (MHz)", "gpu_clock_avg_mhz", "{:.0f}", None),
        ("VRAM (MiB)", "vram_avg_mib", "{:.0f}", None),
        ("GPU Power (W)", "gpu_power_avg_w", "{:.1f}", None),
    ]
    lines = [
        f"# Benchmark Anno 117 : {base_label} vs {tgt_label}",
        "",
        f"| Métrique | {base_label} | {tgt_label} | Delta |",
        "|---|---:|---:|---:|",
    ]
    for title, key, fmt, hib in rows:
        b = base_sum.get(key, 0.0)
        t = tgt_sum.get(key, 0.0)
        delta = "-" if hib is None else format_delta(b, t, hib)
        lines.append(f"| {title} | {fmt.format(b)} | {fmt.format(t)} | {delta} |")

    lines.extend([
        "",
        f"## Diagnostic Coquille ({tgt_label})",
        f"- Direct scanout activé (événements) : {int(tgt_sum.get('scanout_active', 0))}",
        f"- Direct scanout échoué (événements) : {int(tgt_sum.get('scanout_failed', 0))}",
        f"- Commutations VRR : {int(tgt_sum.get('vrr_switches', 0))}",
    ])
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description="Compare MangoHud benchmark runs.")
    parser.add_argument("--baseline", required=True, type=Path, help="Baseline directory or CSV (e.g. GNOME)")
    parser.add_argument("--target", required=True, type=Path, help="Target directory or CSV (e.g. Coquille)")
    parser.add_argument("--baseline-label", default="GNOME", help="Label for baseline")
    parser.add_argument("--target-label", default="Coquille", help="Label for target")
    parser.add_argument("--markdown", type=Path, help="Write Markdown report to path")
    parser.add_argument("--json", type=Path, help="Write raw JSON summary to path")
    args = parser.parse_args()

    base_runs, base_sum = aggregate_runs(args.baseline_label, args.baseline)
    tgt_runs, tgt_sum = aggregate_runs(args.target_label, args.target)

    if not base_runs:
        print(f"Error: no valid MangoHud CSV files found in {args.baseline}", file=sys.stderr)
        return 1
    if not tgt_runs:
        print(f"Error: no valid MangoHud CSV files found in {args.target}", file=sys.stderr)
        return 1

    md = render_markdown(args.baseline_label, base_sum, args.target_label, tgt_sum)
    print(md)

    if args.markdown:
        args.markdown.parent.mkdir(parents=True, exist_ok=True)
        args.markdown.write_text(md, encoding="utf-8")

    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "baseline": {"label": args.baseline_label, "summary": base_sum, "runs": [asdict(r) for r in base_runs]},
            "target": {"label": args.target_label, "summary": tgt_sum, "runs": [asdict(r) for r in tgt_runs]},
        }
        args.json.write_text(json.dumps(payload, indent=2), encoding="utf-8")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
