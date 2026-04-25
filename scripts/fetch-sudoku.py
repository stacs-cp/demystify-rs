#!/usr/bin/env python3
"""Fetch sudoku puzzles from puzzle-sudoku.com and convert to .param files."""

import re
import sys
import time
import urllib.request

DIFFICULTIES = [
    ("basic", 0),
    ("easy", 1),
    ("intermediate", 2),
    ("advanced", 3),
    ("extreme", 4),
    ("evil", 5),
]
INSTANCES_PER_CATEGORY = 20

BASE_URL = "https://www.puzzle-sudoku.com/?size={size}"
OUTPUT_DIR = "eprime/sudoku/puzzle-sudoku-com"


def decode_task(task_str, block_w, block_h):
    """Decode the task string into a 9x9 grid.

    Digits are clue values; underscores separate adjacent digits.
    Letters a-z represent runs of empty cells (a=1, b=2, ..., z=26).
    """
    n = block_w * block_h  # 9 for standard 3x3
    cells = []
    i = 0
    while i < len(task_str):
        ch = task_str[i]
        if ch == "_":
            i += 1
            continue
        if ch.isdigit():
            # Could be multi-digit for larger grids, but for 3x3 always single
            num = ""
            while i < len(task_str) and task_str[i].isdigit():
                num += task_str[i]
                i += 1
            cells.append(int(num))
            continue
        if ch.isalpha():
            cells.extend([0] * (ord(ch) - ord("a") + 1))
            i += 1
            continue
        raise ValueError(f"Unexpected character in task: {ch!r}")

    expected = n * n
    if len(cells) != expected:
        raise ValueError(
            f"Decoded {len(cells)} cells but expected {expected} for {n}x{n}"
        )

    return [cells[r * n : (r + 1) * n] for r in range(n)]


def grid_to_param(grid):
    """Convert a grid to .param file content (Essence Prime format)."""
    rows = []
    for row in grid:
        rows.append("    [" + ",".join(str(c) for c in row) + "]")
    body = ",\n".join(rows)
    return f"language ESSENCE' 1.0\n\nletting fixed be \n[\n{body}\n]\n"


def fetch_puzzle(url):
    """Fetch a puzzle page and extract task string, puzzle ID, and dimensions."""
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    with urllib.request.urlopen(req) as resp:
        html = resp.read().decode("utf-8")

    task_match = re.search(r"var task = '([^']*)'", html)
    if not task_match:
        raise ValueError("Could not find task variable")

    pid_match = re.search(r'id="puzzleID">([^<]+)<', html)
    puzzle_id = pid_match.group(1).strip() if pid_match else "unknown"

    w_match = re.search(r"puzzleWidth:\s*(\d+)", html)
    h_match = re.search(r"puzzleHeight:\s*(\d+)", html)
    if not w_match or not h_match:
        raise ValueError("Could not find puzzle dimensions")

    return (
        task_match.group(1),
        puzzle_id,
        int(w_match.group(1)),
        int(h_match.group(1)),
    )


def main():
    import os

    os.makedirs(OUTPUT_DIR, exist_ok=True)

    all_instances = []

    for diff_name, size_idx in DIFFICULTIES:
        url = BASE_URL.format(size=size_idx)
        seen_ids = set()
        instances = []
        attempts = 0
        max_attempts = INSTANCES_PER_CATEGORY * 4

        print(f"Fetching {diff_name}...", file=sys.stderr)

        while len(instances) < INSTANCES_PER_CATEGORY and attempts < max_attempts:
            attempts += 1
            try:
                task_str, puzzle_id, bw, bh = fetch_puzzle(url)
            except Exception as e:
                print(f"  attempt {attempts}: error: {e}", file=sys.stderr)
                time.sleep(1)
                continue

            if puzzle_id in seen_ids:
                time.sleep(0.3)
                continue

            seen_ids.add(puzzle_id)
            grid = decode_task(task_str, bw, bh)
            num = len(instances) + 1
            filename = f"{diff_name}-{num:02d}.param"
            filepath = os.path.join(OUTPUT_DIR, filename)

            param_content = (
                f"$ Source: {url}\n"
                f"$ Puzzle ID: {puzzle_id}\n"
                + grid_to_param(grid)
            )

            with open(filepath, "w") as f:
                f.write(param_content)

            instances.append((filename, puzzle_id))
            print(
                f"  [{num:2d}/{INSTANCES_PER_CATEGORY}] {filename}  (ID: {puzzle_id})",
                file=sys.stderr,
            )
            time.sleep(0.5)

        all_instances.extend(instances)

        if len(instances) < INSTANCES_PER_CATEGORY:
            print(
                f"  WARNING: only got {len(instances)}/{INSTANCES_PER_CATEGORY} "
                f"unique instances for {diff_name}",
                file=sys.stderr,
            )

    # Write README
    readme_path = os.path.join(OUTPUT_DIR, "README.md")
    with open(readme_path, "w") as f:
        f.write("# Sudoku puzzles from puzzle-sudoku.com\n\n")
        f.write(
            f"These {len(all_instances)} standard 9x9 (3x3 block) sudoku puzzles\n"
            "were scraped from <https://www.puzzle-sudoku.com/> by fetching each\n"
            "difficulty page and extracting the `var task = '...'` JavaScript variable.\n"
            "Each is an algorithmically-generated instance.\n\n"
            "Use for benchmarking; attribute to <https://www.puzzle-sudoku.com/>.\n\n"
        )
        f.write("## Categories\n\n")
        f.write("| Difficulty | Size Index | Count |\n")
        f.write("|---|---|---|\n")
        for diff_name, size_idx in DIFFICULTIES:
            count = sum(1 for fn, _ in all_instances if fn.startswith(diff_name))
            f.write(f"| {diff_name.title()} | {size_idx} | {count} |\n")

        f.write("\n## Files\n\n")
        f.write("| File | Puzzle ID |\n")
        f.write("|---|---|\n")
        for filename, pid in all_instances:
            f.write(f"| `{filename}` | {pid} |\n")

    print(
        f"\nDone. {len(all_instances)} puzzles saved to {OUTPUT_DIR}/", file=sys.stderr
    )


if __name__ == "__main__":
    main()
