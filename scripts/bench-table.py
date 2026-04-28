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
    ("Futoshiki-OA", "futoshiki-oa", [
        "4x4-easy",
        "5x5-easy", "5x5-normal", "5x5-hard",
        "9x9-easy", "9x9-normal", "9x9-hard",
    ]),
    ("Tents-OA", "tents-oa", [
        "6x6-easy", "6x6-hard",
        "10x10-easy", "10x10-hard",
    ]),
]


def read_times(path):
    """Read the key=value time file. Returns dict with wall, user, sys, rss_kb."""
    d = {}
    try:
        for line in open(path):
            line = line.strip()
            if "=" in line:
                k, v = line.split("=", 1)
                d[k] = v
    except FileNotFoundError:
        pass
    # Also handle old-format single-number files.
    if not d:
        try:
            d["wall"] = open(path).read().strip()
        except (FileNotFoundError, ValueError):
            pass
    return d


def parse_seconds(s):
    """Parse a time string like '1.23' or '1:23.45' into seconds."""
    if s is None:
        return None
    try:
        if ":" in s:
            parts = s.split(":")
            return float(parts[0]) * 60 + float(parts[1])
        return float(s)
    except ValueError:
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


def step_count(stderr_path):
    try:
        text = open(stderr_path).read()
    except FileNotFoundError:
        return None
    steps = re.findall(r"(\d+) steps,", text)
    return int(steps[-1]) + 1 if steps else None


def collect(outdir, prefix, diff, method):
    """Collect times, CPU times, and max-MUS-sizes across instances 01–20."""
    wall_times = []
    cpu_times = []
    mus_maxes = []
    steps_list = []
    conjure_times = []
    setup_times = []
    solve_times = []
    for i in range(1, 21):
        nn = f"{i:02d}"
        tag = f"{prefix}-{diff}-{nn}.{method}"
        d = read_times(os.path.join(outdir, f"{tag}.time"))
        w = parse_seconds(d.get("wall"))
        u = parse_seconds(d.get("user"))
        s = parse_seconds(d.get("sys"))
        m = max_mus_size(os.path.join(outdir, f"{tag}.stderr"))
        sc = step_count(os.path.join(outdir, f"{tag}.stderr"))
        tc = parse_seconds(d.get("t_conjure"))
        ts = parse_seconds(d.get("t_setup"))
        tv = parse_seconds(d.get("t_solve"))
        if w is not None:
            wall_times.append(w)
        if u is not None and s is not None:
            cpu_times.append(u + s)
        if m is not None:
            mus_maxes.append(m)
        if sc is not None:
            steps_list.append(sc)
        if tc is not None:
            conjure_times.append(tc)
        if ts is not None:
            setup_times.append(ts)
        if tv is not None:
            solve_times.append(tv)
    return wall_times, cpu_times, mus_maxes, steps_list, conjure_times, setup_times, solve_times


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


METHODS = ["standard", "core", "core+mus"]


def generate(outdir):
    rows = []
    for class_name, prefix, difficulties in CLASSES:
        class_rows = []
        for diff in difficulties:
            row = {"diff": diff}
            for method in METHODS:
                wt, ct, mm, sc, conj, stp, slv = collect(outdir, prefix, diff, method)
                row[method] = {
                    "wall": mean_or_none(wt),
                    "cpu": mean_or_none(ct),
                    "max_mus": mean_or_none(mm),
                    "steps": mean_or_none(sc),
                    "t_conjure": mean_or_none(conj),
                    "t_setup": mean_or_none(stp),
                    "t_solve": mean_or_none(slv),
                    "n": len(wt),
                }
            class_rows.append(row)
        rows.append((class_name, class_rows))
    return rows


def latex(rows):
    lines = []
    lines.append(r"\begin{table}[t]")
    lines.append(r"\centering")
    lines.append(r"\caption{Comparison of MUS methods"
                 r" (mean over 20 instances).}")
    lines.append(r"\label{tab:mus-comparison}")
    lines.append(r"\begin{tabular}{ll rrr rrr}")
    lines.append(r"\toprule")
    lines.append(r" & & \multicolumn{3}{c}{CPU time (s)}"
                 r" & \multicolumn{3}{c}{Max MUS size} \\")
    lines.append(r"\cmidrule(lr){3-5} \cmidrule(lr){6-8}")
    lines.append(r"Class & Difficulty"
                 r" & MUS & Core & C+M"
                 r" & MUS & Core & C+M \\")
    lines.append(r"\midrule")

    for i, (class_name, class_rows) in enumerate(rows):
        if i > 0:
            lines.append(r"\addlinespace")
        for j, row in enumerate(class_rows):
            label = class_name if j == 0 else ""
            s = row.get("standard", {})
            c = row.get("core", {})
            h = row.get("core+mus", {})
            lines.append(
                f"  {label} & {row['diff']}"
                f" & {fmt_time(s.get('cpu'))} & {fmt_time(c.get('cpu'))} & {fmt_time(h.get('cpu'))}"
                f" & {fmt_mus(s.get('max_mus'))} & {fmt_mus(c.get('max_mus'))} & {fmt_mus(h.get('max_mus'))} \\\\"
            )

    lines.append(r"\bottomrule")
    lines.append(r"\end{tabular}")
    lines.append(r"\end{table}")
    return "\n".join(lines)


def plain(rows):
    lines = []
    hdr = (f"{'Class':<12} {'Difficulty':<14}"
           f" {'Conj':>6} {'Setup':>6}"
           f" {'MUS-s':>7} {'Core-s':>7} {'C+M-s':>7}"
           f" {'MUS-w':>7} {'Core-w':>7} {'C+M-w':>7}"
           f" {'MUS-c':>7} {'Core-c':>7} {'C+M-c':>7}"
           f" {'MUS-m':>6} {'Core-m':>6} {'C+M-m':>6}"
           f" {'n':>3}")
    lines.append(hdr)
    lines.append("-" * len(hdr))
    for class_name, class_rows in rows:
        for j, row in enumerate(class_rows):
            label = class_name if j == 0 else ""
            s = row.get("standard", {})
            c = row.get("core", {})
            h = row.get("core+mus", {})
            n = max(s.get("n", 0), c.get("n", 0), h.get("n", 0))
            # conjure/setup are the same across methods; take from whichever has data
            t_conj = s.get("t_conjure") or c.get("t_conjure") or h.get("t_conjure")
            t_setup = s.get("t_setup") or c.get("t_setup") or h.get("t_setup")
            lines.append(
                f"{label:<12} {row['diff']:<14}"
                f" {fmt_time(t_conj):>6} {fmt_time(t_setup):>6}"
                f" {fmt_time(s.get('t_solve')):>7} {fmt_time(c.get('t_solve')):>7} {fmt_time(h.get('t_solve')):>7}"
                f" {fmt_time(s.get('wall')):>7} {fmt_time(c.get('wall')):>7} {fmt_time(h.get('wall')):>7}"
                f" {fmt_time(s.get('cpu')):>7} {fmt_time(c.get('cpu')):>7} {fmt_time(h.get('cpu')):>7}"
                f" {fmt_mus(s.get('max_mus')):>6} {fmt_mus(c.get('max_mus')):>6} {fmt_mus(h.get('max_mus')):>6}"
                f" {n:>3}"
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
