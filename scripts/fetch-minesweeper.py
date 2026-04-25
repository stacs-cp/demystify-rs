#!/usr/bin/env python3
"""Fetch pen-and-paper minesweeper puzzles from puzzle-minesweeper.com."""

import re
import sys
import time
import urllib.request

CATEGORIES = [
    ("5x5-easy", 5, 5),
    ("5x5-hard", 5, 5),
    ("7x7-easy", 7, 7),
    ("7x7-hard", 7, 7),
    ("10x10-easy", 10, 10),
    ("10x10-hard", 10, 10),
    ("15x15-easy", 15, 15),
    ("15x15-hard", 15, 15),
    ("20x20-easy", 20, 20),
    ("20x20-hard", 20, 20),
]
INSTANCES_PER_CATEGORY = 20

BASE_URL = "https://www.puzzle-minesweeper.com/minesweeper-{category}/"
OUTPUT_DIR = "eprime/ppminesweeper/puzzle-minesweeper-com"


def decode_task(task_str, width, height):
    """Decode task string into a grid.

    Digits 0-8: clue cell (number of adjacent mines).
    Letters a-z: run of unknown cells (a=1, b=2, ..., z=26).
    Returns grid with -1 for unknown, 0-8 for clues.
    """
    cells = []
    for ch in task_str:
        if ch.isdigit():
            cells.append(int(ch))
        elif ch.isalpha() and ch.islower():
            cells.extend([-1] * (ord(ch) - ord("a") + 1))
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
    """Convert to .param file content."""
    lines = ["language ESSENCE' 1.0"]
    lines.append(f"letting height be {height}")
    lines.append(f"letting width be {width}")
    lines.append("letting grid be [")
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

    for category, exp_w, exp_h in CATEGORIES:
        url = BASE_URL.format(category=category)
        seen_ids = set()
        instances = []
        attempts = 0
        max_attempts = INSTANCES_PER_CATEGORY * 4

        print(f"Fetching {category}...", file=sys.stderr)

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

            if w != exp_w or h != exp_h:
                print(
                    f"  attempt {attempts}: unexpected size {w}x{h}",
                    file=sys.stderr,
                )
                time.sleep(0.3)
                continue

            seen_ids.add(puzzle_id)
            grid = decode_task(task_str, w, h)
            num = len(instances) + 1
            filename = f"{category}-{num:02d}.param"
            filepath = os.path.join(OUTPUT_DIR, filename)

            param_content = (
                f"$ Source: {url}\n"
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
                f"unique instances for {category}",
                file=sys.stderr,
            )

    readme_path = os.path.join(OUTPUT_DIR, "README.md")
    with open(readme_path, "w") as f:
        f.write("# Pen-and-paper minesweeper puzzles from puzzle-minesweeper.com\n\n")
        f.write(
            f"These {len(all_instances)} puzzles were scraped from\n"
            "<https://www.puzzle-minesweeper.com/>. Unlike traditional minesweeper,\n"
            "these are fully solvable by logic alone — no guessing or revealing.\n"
            "Each cell is either a number clue (count of adjacent mines) or unknown.\n\n"
            "Use for benchmarking; attribute to <https://www.puzzle-minesweeper.com/>.\n\n"
        )
        f.write("## Categories\n\n")
        f.write("| Category | Grid | Count |\n")
        f.write("|---|---|---|\n")
        for category, _, _ in CATEGORIES:
            count = sum(1 for fn, _ in all_instances if fn.startswith(category + "-"))
            f.write(f"| {category} | {category.split('-')[0]} | {count} |\n")

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
