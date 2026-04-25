#!/usr/bin/env python3
"""Fetch futoshiki puzzles from puzzle-futoshiki.com and convert to .param files."""

import re
import sys
import time
import urllib.request

CATEGORIES = [
    ("4x4-easy", 4),
    ("5x5-easy", 5),
    ("5x5-normal", 5),
    ("5x5-hard", 5),
    ("7x7-easy", 7),
    ("7x7-normal", 7),
    ("7x7-hard", 7),
    ("9x9-easy", 9),
    ("9x9-normal", 9),
    ("9x9-hard", 9),
]
INSTANCES_PER_CATEGORY = 20

BASE_URL = "https://www.puzzle-futoshiki.com/futoshiki-{category}/"
OUTPUT_DIR = "eprime/futoshiki/puzzle-futoshiki-com"


def decode_task(task_str, n):
    """Decode the futoshiki task string into grid values and lt constraints.

    Task is comma-separated entries, one per cell (row-major).
    Each entry: {digit}{constraint_suffixes}
    Digit: 0 = empty, 1-9 = given.
    Suffixes: R = right < me, L = left < me, U = above < me, D = below < me.
    """
    entries = [e for e in task_str.split(",") if e]
    if len(entries) != n * n:
        raise ValueError(
            f"Expected {n*n} entries but got {len(entries)} for {n}x{n}"
        )

    values = [[0] * n for _ in range(n)]
    lt_list = []

    for idx, entry in enumerate(entries):
        r = idx // n
        c = idx % n
        val = int(entry[0])
        values[r][c] = val
        suffixes = entry[1:]
        for s in suffixes:
            if s == "R":
                # cell to right < this cell => lt[right, this]
                lt_list.append((r + 1, c + 2, r + 1, c + 1))
            elif s == "L":
                # cell to left < this cell => lt[left, this]
                lt_list.append((r + 1, c, r + 1, c + 1))
            elif s == "U":
                # cell above < this cell => lt[above, this]
                lt_list.append((r, c + 1, r + 1, c + 1))
            elif s == "D":
                # cell below < this cell => lt[below, this]
                lt_list.append((r + 2, c + 1, r + 1, c + 1))
            else:
                raise ValueError(f"Unknown suffix {s!r} in entry {entry!r}")

    return values, lt_list


def to_param(values, lt_list, n):
    """Convert to .param file content (Essence Prime format)."""
    lines = ["language ESSENCE' 1.0"]
    lines.append(f"letting grid_size be {n}")
    lines.append("letting values be")
    lines.append("[")
    for i, row in enumerate(values):
        sep = "," if i < n - 1 else ""
        lines.append(
            " [" + ", ".join(str(v) for v in row) + "]" + sep
        )
    lines.append("]")

    lines.append(f"letting lastdx be {len(lt_list) - 1}")
    lines.append("letting lt be")
    lines.append("[")
    for i, (r1, c1, r2, c2) in enumerate(lt_list):
        sep = "," if i < len(lt_list) - 1 else ""
        lines.append(f"[{r1},{c1},{r2},{c2}]" + sep)
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

    for category, expected_n in CATEGORIES:
        url = BASE_URL.format(category=category)
        seen_ids = set()
        instances = []
        attempts = 0
        max_attempts = INSTANCES_PER_CATEGORY * 4

        print(f"Fetching {category}...", file=sys.stderr)

        while len(instances) < INSTANCES_PER_CATEGORY and attempts < max_attempts:
            attempts += 1
            try:
                task_str, puzzle_id, pw, ph = fetch_puzzle(url)
            except Exception as e:
                print(f"  attempt {attempts}: error: {e}", file=sys.stderr)
                time.sleep(1)
                continue

            if puzzle_id in seen_ids:
                time.sleep(0.3)
                continue

            n = pw
            if n != expected_n or pw != ph:
                print(
                    f"  attempt {attempts}: unexpected size {n} (expected {expected_n})",
                    file=sys.stderr,
                )
                time.sleep(0.3)
                continue

            seen_ids.add(puzzle_id)
            values, lt_list = decode_task(task_str, n)
            num = len(instances) + 1
            filename = f"{category}-{num:02d}.param"
            filepath = os.path.join(OUTPUT_DIR, filename)

            param_content = (
                f"$ Source: {url}\n"
                f"$ Puzzle ID: {puzzle_id}\n"
                + to_param(values, lt_list, n)
            )

            with open(filepath, "w") as f:
                f.write(param_content)

            instances.append((filename, puzzle_id))
            print(
                f"  [{num:2d}/{INSTANCES_PER_CATEGORY}] {filename}  "
                f"(ID: {puzzle_id}, {len(lt_list)} constraints)",
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
        f.write("# Futoshiki puzzles from puzzle-futoshiki.com\n\n")
        f.write(
            f"These {len(all_instances)} puzzles were scraped from\n"
            "<https://www.puzzle-futoshiki.com/> by fetching each category page\n"
            "and extracting the `var task = '...'` JavaScript variable.\n"
            "Each is an algorithmically-generated instance.\n\n"
            "Use for benchmarking; attribute to <https://www.puzzle-futoshiki.com/>.\n\n"
        )
        f.write("## Categories\n\n")
        f.write("| Category | Grid | Count |\n")
        f.write("|---|---|---|\n")
        for category, _ in CATEGORIES:
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
