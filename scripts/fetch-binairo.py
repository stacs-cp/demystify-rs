#!/usr/bin/env python3
"""Fetch binairo puzzles from puzzle-binairo.com and convert to .param files."""

import re
import sys
import time
import urllib.request

SIZES = ["6x6", "8x8", "10x10", "14x14", "20x20"]
DIFFICULTIES = ["easy", "hard"]
INSTANCES_PER_CATEGORY = 20

BASE_URL = "https://www.puzzle-binairo.com/binairo-{size}-{diff}/"
OUTPUT_DIR = "eprime/binairo/puzzle-binairo-com"


def decode_task(task_str, width, height):
    """Decode the task string into a 2D grid.

    0/1 in the string are filled cells; letters a-z represent runs of
    empty cells (a=1, b=2, ..., z=26).
    """
    cells = []
    for ch in task_str:
        if ch in "01":
            cells.append(int(ch))
        elif ch.isalpha():
            cells.extend([2] * (ord(ch) - ord("a") + 1))
        else:
            raise ValueError(f"Unexpected character in task: {ch!r}")

    expected = width * height
    if len(cells) != expected:
        raise ValueError(
            f"Decoded {len(cells)} cells but expected {expected} "
            f"for {width}x{height}"
        )

    return [cells[i * width : (i + 1) * width] for i in range(height)]


def grid_to_param(grid, size):
    """Convert a grid to .param file content."""
    n = len(grid)
    rows = []
    for row in grid:
        rows.append("        [" + ",".join(str(c) for c in row) + "]")
    body = ",\n".join(rows)
    return f"letting n be {n}\nletting initial be\n    [\n{body}\n    ]\n"


def fetch_puzzle(url):
    """Fetch a puzzle page and extract task string and puzzle ID."""
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    with urllib.request.urlopen(req) as resp:
        html = resp.read().decode("utf-8")

    task_match = re.search(r"var task = '([^']*)'", html)
    if not task_match:
        raise ValueError("Could not find task variable")

    pid_match = re.search(r'id="puzzleID">([^<]+)<', html)
    puzzle_id = pid_match.group(1).strip() if pid_match else "unknown"

    size_match = re.search(r"puzzleWidth:\s*(\d+),\s*puzzleHeight:\s*(\d+)", html)
    if not size_match:
        raise ValueError("Could not find puzzle dimensions")

    return task_match.group(1), puzzle_id, int(size_match.group(1)), int(size_match.group(2))


def main():
    import os

    os.makedirs(OUTPUT_DIR, exist_ok=True)

    all_instances = []

    for size in SIZES:
        for diff in DIFFICULTIES:
            url = BASE_URL.format(size=size, diff=diff)
            category = f"{size}-{diff}"
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
                    # Got a duplicate, try again
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
                    + grid_to_param(grid, size)
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

    # Write README
    readme_path = os.path.join(OUTPUT_DIR, "README.md")
    with open(readme_path, "w") as f:
        f.write("# Binairo puzzles from puzzle-binairo.com\n\n")
        f.write(
            f"These {len(all_instances)} puzzles were scraped from\n"
            "<https://www.puzzle-binairo.com/> by fetching each category page\n"
            "and extracting the `var task = '...'` JavaScript variable.\n"
            "Each is an algorithmically-generated instance.\n\n"
            "Use for benchmarking; attribute to <https://www.puzzle-binairo.com/>.\n\n"
        )
        f.write("## Categories\n\n")
        f.write("| Category | Grid | Difficulty | Count |\n")
        f.write("|---|---|---|---|\n")
        for size in SIZES:
            for diff in DIFFICULTIES:
                cat = f"{size}-{diff}"
                count = sum(1 for fn, _ in all_instances if fn.startswith(cat))
                f.write(f"| {cat} | {size} | {diff.title()} | {count} |\n")

        f.write("\n## Files\n\n")
        f.write("| File | Puzzle ID |\n")
        f.write("|---|---|\n")
        for filename, pid in all_instances:
            f.write(f"| `{filename}` | {pid} |\n")

    print(f"\nDone. {len(all_instances)} puzzles saved to {OUTPUT_DIR}/", file=sys.stderr)


if __name__ == "__main__":
    main()
