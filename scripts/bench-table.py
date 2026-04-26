#!/usr/bin/env python3
"""Generate a LaTeX table from bench-experiment.sh results.

Reads results from the experiment output directory and aggregates across
instances (01–20).  Reports mean time and mean max-MUS-size for each
puzzle class × difficulty × method (standard / core).

Usage:
    python3 scripts/bench-table.py <results-dir>            # LaTeX
    python3 scripts/bench-table.py <results-dir> --plain    # terminal
"""

import os
import re
import statistics
import sys

CLASSES = [
    ("Binairo", "binairo", [
        "6x6-easy", "6x6-hard",
        "10x10-easy", "10x10-hard",
        "20x20-easy", "20x20-hard",
    ]),
    ("Sudoku", "sudoku", [
        "basic", "easy", "intermediate", "advanced", "extreme", "evil",
    ]),
    ("Futoshiki", "futoshiki", [
        "4x4-easy",
        "5x5-easy", "5x5-normal", "5x5-hard",
        "9x9-easy", "9x9-normal", "9x9-hard",
    ]),
    ("Akari", "akari", [
        "7x7-easy", "7x7-normal", "7x7-hard",
        "10x10-easy", "10x10-normal", "10x10-hard",
        "14x14-easy", "14x14-normal", "14x14-hard",
    ]),
    ("Minesweeper", "minesweeper", [
        "5x5-easy", "5x5-hard",
        "10x10-easy", "10x10-hard",
        "20x20-easy", "20x20-hard",
    ]),
    ("Tents", "tents", [
        "6x6-easy", "6x6-hard",
        "10x10-easy", "10x10-hard",
    ]),
]


def read_time(path):
    try:
        return float(open(path).read().strip())
    except (FileNotFoundError, ValueError):
        return None


def max_mus_size(stderr_path):
    try:
        text = open(stderr_path).read()
    except FileNotFoundError:
        return None
    if "unexpected argument" in text:
        return None
    sizes = [int(m) for m in re.findall(r"muses of size (\d+)", text)]
    return max(sizes) if sizes else None


def collect(outdir, prefix, diff, method):
    """Collect times and max-MUS-sizes across instances 01–20."""
    times = []
    mus_maxes = []
    for i in range(1, 21):
        nn = f"{i:02d}"
        tag = f"{prefix}-{diff}-{nn}.{method}"
        t = read_time(os.path.join(outdir, f"{tag}.time"))
        m = max_mus_size(os.path.join(outdir, f"{tag}.stderr"))
        if t is not None:
            times.append(t)
        if m is not None:
            mus_maxes.append(m)
    return times, mus_maxes


def mean_or_none(xs):
    return statistics.mean(xs) if xs else None


def fmt_time(t):
    if t is None:
        return "---"
    if t >= 100:
        return f"{t:.0f}"
    if t >= 10:
        return f"{t:.1f}"
    return f"{t:.2f}"


def fmt_mus(m):
    if m is None:
        return "---"
    return f"{m:.1f}"


def generate(outdir):
    rows = []
    for class_name, prefix, difficulties in CLASSES:
        class_rows = []
        for diff in difficulties:
            st, sm = collect(outdir, prefix, diff, "standard")
            ct, cm = collect(outdir, prefix, diff, "core")
            n_found = len(st)
            class_rows.append((
                diff,
                mean_or_none(st),
                mean_or_none(ct),
                mean_or_none(sm),
                mean_or_none(cm),
                n_found,
            ))
        rows.append((class_name, class_rows))
    return rows


def latex(rows):
    lines = []
    lines.append(r"\begin{table}[t]")
    lines.append(r"\centering")
    lines.append(r"\caption{Comparison of standard MUS search vs.\ raw SAT cores"
                 r" (mean over 20 instances, 50-step cap).}")
    lines.append(r"\label{tab:core-comparison}")
    lines.append(r"\begin{tabular}{ll rr rr}")
    lines.append(r"\toprule")
    lines.append(r" & & \multicolumn{2}{c}{Time (s)} & \multicolumn{2}{c}{Max MUS size} \\")
    lines.append(r"\cmidrule(lr){3-4} \cmidrule(lr){5-6}")
    lines.append(r"Class & Difficulty & Standard & Core & Standard & Core \\")
    lines.append(r"\midrule")

    for i, (class_name, class_rows) in enumerate(rows):
        if i > 0:
            lines.append(r"\addlinespace")
        for j, (diff, nt, ct, nm, cm, n) in enumerate(class_rows):
            label = class_name if j == 0 else ""
            lines.append(
                f"  {label} & {diff} & {fmt_time(nt)} & {fmt_time(ct)}"
                f" & {fmt_mus(nm)} & {fmt_mus(cm)} \\\\"
            )

    lines.append(r"\bottomrule")
    lines.append(r"\end{tabular}")
    lines.append(r"\end{table}")
    return "\n".join(lines)


def plain(rows):
    lines = []
    hdr = (f"{'Class':<12} {'Difficulty':<14} {'N-time':>7} {'C-time':>7}"
           f"   {'N-max':>6} {'C-max':>6}  {'n':>3}")
    lines.append(hdr)
    lines.append("-" * len(hdr))
    for class_name, class_rows in rows:
        for j, (diff, nt, ct, nm, cm, n) in enumerate(class_rows):
            label = class_name if j == 0 else ""
            lines.append(
                f"{label:<12} {diff:<14} {fmt_time(nt):>7} {fmt_time(ct):>7}"
                f"   {fmt_mus(nm):>6} {fmt_mus(cm):>6}  {n:>3}"
            )
        lines.append("")
    return "\n".join(lines)


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    flags = [a for a in sys.argv[1:] if a.startswith("-")]

    if not args:
        print(f"Usage: {sys.argv[0]} <results-dir> [--plain]", file=sys.stderr)
        sys.exit(1)

    outdir = args[0]
    rows = generate(outdir)
    if "--plain" in flags:
        print(plain(rows))
    else:
        print(latex(rows))
