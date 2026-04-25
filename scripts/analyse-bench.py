#!/usr/bin/env python3
"""Analyse bench-cores.sh output: compare normal vs core-guided solving."""

import os
import re
import sys

OUTDIR = "/tmp/claude/bench-cores"

def parse_run(name, mode):
    prefix = os.path.join(OUTDIR, f"{name}.{mode}")
    time_file = f"{prefix}.time"
    stderr_file = f"{prefix}.stderr"
    stdout_file = f"{prefix}.stdout"

    if not os.path.exists(time_file):
        return None

    with open(time_file) as f:
        elapsed = float(f.read().strip())

    # Count steps from stdout (each non-empty line is a step)
    with open(stdout_file) as f:
        steps = sum(1 for line in f if line.strip())

    # Parse progress lines from stderr to get MUS sizes
    # Format: "{n} steps, just found {count} muses of size {size}, ..."
    mus_sizes = []  # list of (size, count) per step
    with open(stderr_file) as f:
        for line in f:
            m = re.search(r'just found (\d+) muses of size (\d+)', line)
            if m:
                count = int(m.group(1))
                size = int(m.group(2))
                mus_sizes.append((size, count))

    max_size = max((s for s, _ in mus_sizes), default=0)
    max_count = sum(c for s, c in mus_sizes if s == max_size)

    return {
        'time': elapsed,
        'steps': steps,
        'max_mus': max_size,
        'max_mus_count': max_count,
        'mus_sizes': mus_sizes,
    }


def main():
    # Discover puzzle names from files
    names = set()
    for f in os.listdir(OUTDIR):
        if f.endswith('.time'):
            base = f.rsplit('.', 2)[0]  # name.mode.time -> name
            # handle names with dots by splitting on .normal. or .cores.
            for mode in ('normal', 'cores'):
                suffix = f'.{mode}.time'
                if f.endswith(suffix):
                    names.add(f[:-len(suffix)])
    names = sorted(names)

    if not names:
        print(f"No results found in {OUTDIR}")
        sys.exit(1)

    # Header
    print(f"{'Puzzle':<25} {'Time(N)':>8} {'Time(C)':>8} {'Speedup':>8} "
          f"{'Steps(N)':>9} {'Steps(C)':>9} "
          f"{'MaxMUS(N)':>10} {'#(N)':>5} {'MaxMUS(C)':>10} {'#(C)':>5}")
    print("-" * 115)

    total_normal = 0
    total_cores = 0
    wins = 0
    losses = 0
    same_steps = 0
    larger_mus = 0

    for name in names:
        n = parse_run(name, 'normal')
        c = parse_run(name, 'cores')
        if not n or not c:
            continue

        speedup = n['time'] / c['time'] if c['time'] > 0 else float('inf')
        total_normal += n['time']
        total_cores += c['time']

        if speedup > 1.05:
            wins += 1
        elif speedup < 0.95:
            losses += 1

        if n['steps'] == c['steps']:
            same_steps += 1

        if c['max_mus'] > n['max_mus']:
            larger_mus += 1

        speedup_str = f"{speedup:.1f}x"

        print(f"{name:<25} {n['time']:>7.1f}s {c['time']:>7.1f}s {speedup_str:>8} "
              f"{n['steps']:>9} {c['steps']:>9} "
              f"{n['max_mus']:>10} {n['max_mus_count']:>5} {c['max_mus']:>10} {c['max_mus_count']:>5}")

    print("-" * 115)
    overall = total_normal / total_cores if total_cores > 0 else 0
    print(f"{'TOTAL':<25} {total_normal:>7.1f}s {total_cores:>7.1f}s {overall:.1f}x")
    print()
    print(f"Puzzles where cores faster (>5%): {wins}/{len(names)}")
    print(f"Puzzles where cores slower (>5%): {losses}/{len(names)}")
    print(f"Same step count:                  {same_steps}/{len(names)}")
    print(f"Cores produced larger max MUS:    {larger_mus}/{len(names)}")


if __name__ == '__main__':
    main()
