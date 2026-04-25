#!/usr/bin/env python3
"""Fetch tents-and-trees puzzles from puzzle-tents.com."""

import re
import sys
import time
import urllib.request

CATEGORIES = [
    ("6x6-easy", 0, 6),
    ("6x6-hard", 1, 6),
    ("8x8-easy", 2, 8),
    ("8x8-hard", 3, 8),
    ("10x10-easy", 4, 10),
    ("10x10-hard", 5, 10),
    ("15x15-easy", 6, 15),
    ("15x15-hard", 7, 15),
]
INSTANCES_PER_CATEGORY = 20

BASE_URL = "https://www.puzzle-tents.com/?size={size}"
OUTPUT_DIR = "eprime/tents/puzzle-tents-com"


def decode_task(task_str, width, height):
    """Decode tents task string into tree grid, col sums, and row sums.

    Format: <grid_string>,<col_0>,...,<col_{w-1}>,<row_0>,...,<row_{h-1}>

    Grid string encoding:
      Letters a-y: skip N empty cells then place a tree (N = charCode - 96).
      z: skip 25 cells, NO tree (pure spacer).
      _: place tree at current position (skip 0).
    """
    parts = task_str.split(",")
    grid_str = parts[0]
    counts = [int(x) for x in parts[1:]]
    col_sums = counts[:width]
    row_sums = counts[width : width + height]

    if len(col_sums) != width or len(row_sums) != height:
        raise ValueError(
            f"Expected {width} col sums and {height} row sums, "
            f"got {len(col_sums)} and {len(row_sums)}"
        )

    tree_positions = []
    total = width * height
    pos = 0
    for ch in grid_str:
        if ch != "_":
            pos += ord(ch) - ord("a") + 1  # decodeChar: a=1, b=2, ...
        if ch == "z":
            pos -= 1  # z is pure spacer (net advance 25)
        elif pos < total:
            tree_positions.append(pos)
            pos += 1

    treecount = len(tree_positions)
    trees = [[0] * width for _ in range(height)]
    for i, p in enumerate(tree_positions):
        r = p // width
        c = p % width
        if r >= height or c >= width:
            raise ValueError(
                f"Tree position {p} (row={r}, col={c}) out of bounds "
                f"for {width}x{height}"
            )
        trees[r][c] = i + 1  # 1-indexed tree IDs

    expected_tents = sum(row_sums)
    if treecount != expected_tents:
        raise ValueError(
            f"Decoded {treecount} trees but row sums total {expected_tents} "
            f"grid_str={grid_str!r} positions={tree_positions}"
        )

    return trees, treecount, col_sums, row_sums


def to_param(trees, treecount, col_sums, row_sums, width, height):
    """Convert to .param file content matching tents.eprime format."""
    lines = [f"letting x be {height}"]
    lines.append(f"letting y be {width}")
    lines.append(f"letting treecount be {treecount}")
    lines.append("")
    lines.append(
        "letting col_sums be [" + ",".join(str(s) for s in col_sums) + "]"
    )
    lines.append(
        "letting row_sums be [" + ",".join(str(s) for s in row_sums) + "]"
    )
    lines.append("")
    lines.append("letting trees be")
    lines.append("[")
    for i, row in enumerate(trees):
        sep = "," if i < height - 1 else ""
        lines.append(
            "    [" + ",".join(f"{v:2d}" for v in row) + "]" + sep
        )
    lines.append("]")
    return "\n".join(lines) + "\n"


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

    for cat_name, size_idx, expected_n in CATEGORIES:
        url = BASE_URL.format(size=size_idx)
        seen_ids = set()
        instances = []
        attempts = 0
        max_attempts = INSTANCES_PER_CATEGORY * 4

        print(f"Fetching {cat_name}...", file=sys.stderr)

        while len(instances) < INSTANCES_PER_CATEGORY and attempts < max_attempts:
            attempts += 1
            try:
                task_str, puzzle_id, w, h = fetch_puzzle(url)
            except Exception as e:
                print(f"  attempt {attempts}: error: {e}", file=sys.stderr)
                time.sleep(1)
                continue

            if puzzle_id in seen_ids:
                time.sleep(0.3)
                continue

            if w != expected_n or h != expected_n:
                print(
                    f"  attempt {attempts}: unexpected size {w}x{h}",
                    file=sys.stderr,
                )
                time.sleep(0.3)
                continue

            seen_ids.add(puzzle_id)
            trees, treecount, col_sums, row_sums = decode_task(task_str, w, h)
            num = len(instances) + 1
            filename = f"{cat_name}-{num:02d}.param"
            filepath = os.path.join(OUTPUT_DIR, filename)

            param_content = (
                f"$ Source: {url}\n"
                f"$ Category: {cat_name}\n"
                f"$ Puzzle ID: {puzzle_id}\n"
                + to_param(trees, treecount, col_sums, row_sums, w, h)
            )

            with open(filepath, "w") as f:
                f.write(param_content)

            instances.append((filename, puzzle_id))
            print(
                f"  [{num:2d}/{INSTANCES_PER_CATEGORY}] {filename}  "
                f"(ID: {puzzle_id}, {treecount} trees)",
                file=sys.stderr,
            )
            time.sleep(0.5)

        all_instances.extend(instances)

        if len(instances) < INSTANCES_PER_CATEGORY:
            print(
                f"  WARNING: only got {len(instances)}/{INSTANCES_PER_CATEGORY} "
                f"unique instances for {cat_name}",
                file=sys.stderr,
            )

    readme_path = os.path.join(OUTPUT_DIR, "README.md")
    with open(readme_path, "w") as f:
        f.write("# Tents and Trees puzzles from puzzle-tents.com\n\n")
        f.write(
            f"These {len(all_instances)} puzzles were scraped from\n"
            "<https://www.puzzle-tents.com/>. Place tents adjacent to trees\n"
            "such that no two tents touch (even diagonally) and row/column\n"
            "counts are satisfied.\n\n"
            "Use for benchmarking; attribute to <https://www.puzzle-tents.com/>.\n\n"
        )
        f.write("## Categories\n\n")
        f.write("| Category | Grid | Count |\n")
        f.write("|---|---|---|\n")
        for cat_name, _, _ in CATEGORIES:
            count = sum(1 for fn, _ in all_instances if fn.startswith(cat_name + "-"))
            f.write(f"| {cat_name} | {cat_name.split('-')[0]} | {count} |\n")

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
