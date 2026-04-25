#!/usr/bin/env python3
"""Fetch Light Up (Akari) puzzles from puzzle-light-up.com and convert to .param files."""

import re
import sys
import time
import urllib.request

CATEGORIES = [
    ("7x7-easy", 0, 7),
    ("7x7-normal", 1, 7),
    ("7x7-hard", 2, 7),
    ("10x10-easy", 3, 10),
    ("10x10-normal", 4, 10),
    ("10x10-hard", 5, 10),
    ("14x14-easy", 6, 14),
    ("14x14-normal", 7, 14),
    ("14x14-hard", 8, 14),
]
INSTANCES_PER_CATEGORY = 20

BASE_URL = "https://www.puzzle-light-up.com/?size={size}"
OUTPUT_DIR = "eprime/akari/puzzle-light-up-com"


def decode_task(task_str, width, height):
    """Decode the Light Up task string into a grid.

    Digits 0-4: numbered black cell (that many adjacent bulbs required).
    B: unnumbered black cell.
    Letters a-z: run of empty (white) cells (a=1, b=2, ..., z=26).

    Returns grid using the akari.eprime encoding:
      -1 = black cell (no number)
       0-4 = numbered black cell
       5 = white cell
    """
    cells = []
    for ch in task_str:
        if ch in "01234":
            cells.append(int(ch))
        elif ch == "B":
            cells.append(-1)
        elif ch.isalpha() and ch.islower():
            cells.extend([5] * (ord(ch) - ord("a") + 1))
        else:
            raise ValueError(f"Unexpected character in task: {ch!r}")

    expected = width * height
    if len(cells) != expected:
        raise ValueError(
            f"Decoded {len(cells)} cells but expected {expected} "
            f"for {width}x{height}"
        )

    return [cells[i * width : (i + 1) * width] for i in range(height)]


def grid_to_param(grid, height, width):
    """Convert a grid to .param file content (Essence Prime format)."""
    lines = ["language ESSENCE' 1.0"]
    lines.append(f"letting height be {height}")
    lines.append(f"letting width be {width}")
    lines.append("letting start_grid be [")
    for i, row in enumerate(grid):
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
                    f"  attempt {attempts}: unexpected size {w}x{h} "
                    f"(expected {expected_n}x{expected_n})",
                    file=sys.stderr,
                )
                time.sleep(0.3)
                continue

            seen_ids.add(puzzle_id)
            grid = decode_task(task_str, w, h)
            num = len(instances) + 1
            filename = f"{cat_name}-{num:02d}.param"
            filepath = os.path.join(OUTPUT_DIR, filename)

            param_content = (
                f"$ Source: {url}\n"
                f"$ Category: {cat_name}\n"
                f"$ Puzzle ID: {puzzle_id}\n"
                + grid_to_param(grid, h, w)
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
                f"unique instances for {cat_name}",
                file=sys.stderr,
            )

    readme_path = os.path.join(OUTPUT_DIR, "README.md")
    with open(readme_path, "w") as f:
        f.write("# Light Up (Akari) puzzles from puzzle-light-up.com\n\n")
        f.write(
            f"These {len(all_instances)} puzzles were scraped from\n"
            "<https://www.puzzle-light-up.com/> by fetching each category page\n"
            "and extracting the `var task = '...'` JavaScript variable.\n"
            "Each is an algorithmically-generated instance.\n\n"
            "Use for benchmarking; attribute to <https://www.puzzle-light-up.com/>.\n\n"
        )
        f.write("## Categories\n\n")
        f.write("| Category | Grid | Count |\n")
        f.write("|---|---|---|\n")
        for cat_name, _, _ in CATEGORIES:
            count = sum(1 for fn, _ in all_instances if fn.startswith(cat_name + "-"))
            f.write(f"| {cat_name} | {cat_name.split('-')[0]}x{cat_name.split('-')[0]} | {count} |\n")

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
