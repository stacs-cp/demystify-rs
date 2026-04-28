#!/usr/bin/env python3
"""Extract puzzles from the old Python demystify's sudoku-wiki.py and generate
Essence Prime .param files for the sudoku-domains / xsudoku-domains / jigsaw-domains
models.

Usage:
    python3 scripts/extract-sudokuwiki.py

The script reads the original source file, parses all dotest() calls with their
preceding sudokudoms assignments, and writes .param files into:
    eprime/sudoku/sudokuwiki/
    eprime/xsudoku/sudokuwiki/
    eprime/jigsaw/sudokuwiki/
"""

import ast
import os
import re
import sys
import textwrap

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SOURCE = os.path.join(
    REPO_ROOT, "..", "..", "demystify-ijcai-final", "examples", "sudoku-wiki.py",
)

OUTPUT_DIRS = {
    "sudoku": os.path.join(REPO_ROOT, "eprime", "sudoku", "sudokuwiki-domains"),
    "xsudoku": os.path.join(REPO_ROOT, "eprime", "xsudoku", "sudokuwiki"),
    "jigsaw": os.path.join(REPO_ROOT, "eprime", "jigsaw", "sudokuwiki"),
}

# Jigsaw region strings from the source file.
JIGSAW_REGIONS = {
    "jigsawH": "111223333112223633112225633411525666444555666444585996774588899774788899777788999",
    "zigzag":  "111123333114122333144522236144552236447555266487755669487775669888779699888879999",
}


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def slugify(name):
    """Lowercase, replace non-alphanumeric with hyphens, collapse runs."""
    s = name.lower()
    s = re.sub(r"[^a-z0-9]+", "-", s)
    s = s.strip("-")
    return s


def deduplicate_slug(slug, seen):
    """Return a unique slug, appending -2, -3, ... if needed."""
    if slug not in seen:
        seen[slug] = 1
        return slug
    seen[slug] += 1
    return f"{slug}-{seen[slug]}"


def domains_to_allowed(domains):
    """Convert 9x9 list-of-candidates to 9x9x9 binary allowed matrix."""
    rows = []
    for row in domains:
        crow = []
        for cell in row:
            bits = [1 if (d + 1) in cell else 0 for d in range(9)]
            crow.append(bits)
        rows.append(crow)
    return rows


def format_allowed(allowed):
    """Format the allowed matrix as an Essence Prime literal."""
    lines = ["["]
    for i, row in enumerate(allowed):
        row_lines = ["    ["]
        for j, cell in enumerate(row):
            comma = "," if j < 8 else ""
            row_lines.append(f"        [{','.join(str(v) for v in cell)}]{comma}")
        row_lines.append("    ]" + ("," if i < 8 else ""))
        lines.extend(row_lines)
    lines.append("]")
    return "\n".join(lines)


def format_regions(region_str):
    """Format a jigsaw region string as a 9x9 Essence Prime matrix."""
    assert len(region_str) == 81
    lines = ["["]
    for i in range(9):
        vals = [region_str[i * 9 + j] for j in range(9)]
        comma = "," if i < 8 else ""
        lines.append(f"    [{','.join(vals)}]{comma}")
    lines.append("]")
    return "\n".join(lines)


def format_targets(pos_list):
    """Format target deductions as comment lines. Converts 0-indexed to 1-indexed."""
    parts = []
    for (row, col), val in pos_list:
        parts.append(f"({row + 1},{col + 1})!={val}")
    return "; ".join(parts)


def resolve_region_arg(arg_str):
    """Resolve a sudokuarg reference to the actual region string."""
    arg_str = arg_str.strip()
    if arg_str in JIGSAW_REGIONS:
        return JIGSAW_REGIONS[arg_str]
    # It might be a literal string
    if arg_str.startswith('"') or arg_str.startswith("'"):
        return ast.literal_eval(arg_str)
    raise ValueError(f"Unknown jigsaw region arg: {arg_str!r}")


# ---------------------------------------------------------------------------
# Parser — extract dotest calls and their preceding sudokudoms assignments
# ---------------------------------------------------------------------------

def parse_source(path):
    """Parse the sudoku-wiki.py source and extract all puzzle definitions.

    Returns a list of dicts:
        {
            "name": str,
            "domains": list,           # 9x9 list-of-lists
            "pos": list,               # [((r,c), v), ...]
            "sudokutype": str,         # "basic" | "xsudoku" | "jigsaw"
            "sudokuarg": str | None,   # region string for jigsaw
            "technique": str,          # from following printrow()
        }
    """
    with open(path) as f:
        source = f.read()

    # Replace "if False:" with "if True:" so disabled blocks are included.
    # We don't actually exec the source — we just need this for the regex
    # matching to correctly identify the indentation context.

    lines = source.split("\n")

    # We'll do a line-by-line scan to find:
    # 1. sudokudoms = [...] assignments
    # 2. dotest(...) calls
    # 3. printrow("...") calls
    # We skip doSingleStep calls (benchmarking, not puzzle definitions).

    puzzles = []
    current_doms = None
    current_technique_puzzles = []  # indices into puzzles
    all_technique_puzzles = []  # list of (technique_name, [puzzle_indices])
    ungrouped_start = 0

    # Join the source into one string for regex matching.
    # First, flatten multi-line expressions by tracking brackets.
    # Strategy: find each sudokudoms assignment and dotest call using regex,
    # handling the fact that they may span multiple lines.

    # Simpler approach: since the sudokudoms and dotest calls are all on
    # single (very long) lines in this file, we can match line by line.
    # Let's verify this assumption and also handle any multi-line cases.

    # Actually, looking at the source, each sudokudoms and dotest is on a
    # single line (they're very long lines). Let's exploit that.

    # But we still need to handle the `if False:` / `if True:` blocks.
    # Since we're not executing, we just need to NOT skip lines inside
    # `if False:` blocks — we parse everything.

    i = 0
    while i < len(lines):
        line = lines[i]
        stripped = line.strip()

        # Match sudokudoms assignment
        if re.match(r'\s*sudokudoms\s*=\s*\[', stripped):
            # Might span multiple lines — collect until brackets balance
            expr = stripped.split("=", 1)[1].strip()
            depth = expr.count("[") - expr.count("]")
            j = i + 1
            while depth > 0 and j < len(lines):
                expr += lines[j].strip()
                depth = expr.count("[") - expr.count("]")
                j += 1
            try:
                current_doms = ast.literal_eval(expr)
            except (ValueError, SyntaxError) as e:
                print(f"Warning: could not parse sudokudoms at line {i+1}: {e}",
                      file=sys.stderr)
                current_doms = None
            i = j if j > i + 1 else i + 1
            continue

        # Match dotest call (but NOT doSingleStep)
        m = re.match(r'\s*dotest\s*\(', stripped)
        if m:
            # Collect the full call
            call_text = stripped
            depth = call_text.count("(") - call_text.count(")")
            j = i + 1
            while depth > 0 and j < len(lines):
                call_text += " " + lines[j].strip()
                depth = call_text.count("(") - call_text.count(")")
                j += 1

            puzzle = parse_dotest_call(call_text, current_doms, i + 1)
            if puzzle is not None:
                idx = len(puzzles)
                puzzles.append(puzzle)
                current_technique_puzzles.append(idx)

            i = j if j > i + 1 else i + 1
            continue

        # Match printrow call
        m = re.match(r'\s*printrow\s*\(\s*"([^"]*)"\s*\)', stripped)
        if m:
            technique_name = m.group(1)
            if current_technique_puzzles:
                all_technique_puzzles.append(
                    (technique_name, list(current_technique_puzzles))
                )
                current_technique_puzzles = []
            i += 1
            continue

        i += 1

    # Any remaining puzzles without a printrow get "Custom" technique
    if current_technique_puzzles:
        all_technique_puzzles.append(("Custom", list(current_technique_puzzles)))

    # Now assign technique names to puzzles
    for technique_name, indices in all_technique_puzzles:
        for idx in indices:
            puzzles[idx]["technique"] = technique_name

    # Chris Bonus puzzles are standard sudoku, not part of a named technique.
    for p in puzzles:
        if p["name"].startswith("Chris Bonus"):
            p["technique"] = "Custom"

    return puzzles


def parse_dotest_call(call_text, current_doms, line_num):
    """Parse a dotest(...) call string and return a puzzle dict."""
    # dotest(sudokudoms, "Name", [((r,c),v)], sudokutype=..., sudokuarg=...)
    # or
    # dotest(doms, "Name", [((r,c),v)])

    # Extract the arguments. The first arg is a variable name (sudokudoms/doms),
    # the second is a string, the third is a list of tuples,
    # and there may be keyword args.

    # Strip the "dotest(" prefix and trailing ")"
    inner = call_text.strip()
    assert inner.startswith("dotest("), f"Bad dotest call: {inner[:50]}"
    inner = inner[len("dotest("):]
    if inner.endswith(")"):
        inner = inner[:-1]

    # The first argument is a variable name — skip it
    # Find the first comma that's not inside brackets
    depth = 0
    first_comma = None
    for ci, ch in enumerate(inner):
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        elif ch == "," and depth == 0:
            first_comma = ci
            break

    if first_comma is None:
        print(f"Warning: could not parse dotest at line {line_num}", file=sys.stderr)
        return None

    # Second argument: the name string
    rest = inner[first_comma + 1:].strip()
    # Find the name string
    name_match = re.match(r'\s*"([^"]*)"', rest)
    if not name_match:
        name_match = re.match(r"\s*'([^']*)'", rest)
    if not name_match:
        print(f"Warning: could not parse name in dotest at line {line_num}",
              file=sys.stderr)
        return None

    name = name_match.group(1)
    rest = rest[name_match.end():]

    # Skip comma
    rest = rest.lstrip()
    if rest.startswith(","):
        rest = rest[1:].lstrip()

    # Third argument: the pos list — find the matching bracket
    if not rest.startswith("["):
        print(f"Warning: expected pos list in dotest at line {line_num}",
              file=sys.stderr)
        return None

    depth = 0
    end_bracket = None
    for ci, ch in enumerate(rest):
        if ch == "[":
            depth += 1
        elif ch == "]":
            depth -= 1
            if depth == 0:
                end_bracket = ci
                break

    if end_bracket is None:
        print(f"Warning: unmatched bracket in pos list at line {line_num}",
              file=sys.stderr)
        return None

    pos_str = rest[:end_bracket + 1]
    try:
        pos = ast.literal_eval(pos_str)
    except (ValueError, SyntaxError) as e:
        print(f"Warning: could not parse pos list at line {line_num}: {e}",
              file=sys.stderr)
        return None

    rest = rest[end_bracket + 1:].strip()

    # Parse keyword arguments
    sudokutype = "basic"
    sudokuarg = None

    # Look for sudokutype=...
    type_match = re.search(
        r'sudokutype\s*=\s*demystify\.buildpuz\.(\w+)', rest
    )
    if type_match:
        type_name = type_match.group(1)
        if type_name == "basicSudoku":
            sudokutype = "basic"
        elif type_name == "basicXSudoku":
            sudokutype = "xsudoku"
        elif type_name == "buildJigsaw":
            sudokutype = "jigsaw"
        else:
            print(f"Warning: unknown sudokutype {type_name} at line {line_num}",
                  file=sys.stderr)

    # Look for sudokuarg=...
    arg_match = re.search(r'sudokuarg\s*=\s*(\w+)', rest)
    if arg_match:
        sudokuarg = resolve_region_arg(arg_match.group(1))

    if current_doms is None:
        print(f"Warning: no sudokudoms before dotest at line {line_num}",
              file=sys.stderr)
        return None

    # Validate domains
    if len(current_doms) != 9:
        print(f"Warning: sudokudoms has {len(current_doms)} rows at line {line_num}",
              file=sys.stderr)
        return None
    for row in current_doms:
        if len(row) != 9:
            print(f"Warning: sudokudoms row has {len(row)} cells at line {line_num}",
                  file=sys.stderr)
            return None

    return {
        "name": name,
        "domains": current_doms,
        "pos": pos,
        "sudokutype": sudokutype,
        "sudokuarg": sudokuarg,
        "technique": "Custom",  # will be overwritten
    }


# ---------------------------------------------------------------------------
# Writer
# ---------------------------------------------------------------------------

def write_param(puzzle, filepath):
    """Write a single .param file."""
    allowed = domains_to_allowed(puzzle["domains"])

    lines = []
    lines.append("language ESSENCE' 1.0")
    lines.append("")
    lines.append("$ Source: https://www.sudokuwiki.org/")
    lines.append(f"$ Technique: {puzzle['technique']}")
    lines.append(f"$ Name: {puzzle['name']}")
    lines.append(f"$ Target deduction: {format_targets(puzzle['pos'])}")
    lines.append("")
    lines.append("letting allowed be")
    lines.append(format_allowed(allowed))

    if puzzle["sudokutype"] == "jigsaw" and puzzle["sudokuarg"]:
        lines.append("")
        lines.append("letting regions be")
        lines.append(format_regions(puzzle["sudokuarg"]))

    lines.append("")

    with open(filepath, "w") as f:
        f.write("\n".join(lines))


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    if not os.path.exists(SOURCE):
        print(f"Error: source file not found: {SOURCE}", file=sys.stderr)
        sys.exit(1)

    puzzles = parse_source(SOURCE)

    if not puzzles:
        print("Error: no puzzles extracted!", file=sys.stderr)
        sys.exit(1)

    # Create output directories
    for d in OUTPUT_DIRS.values():
        os.makedirs(d, exist_ok=True)

    # Track slug uniqueness per output directory
    slug_counts = {k: {} for k in OUTPUT_DIRS}

    # Group puzzles by type for summary
    counts = {"sudoku": 0, "xsudoku": 0, "jigsaw": 0}
    written = []

    for puzzle in puzzles:
        ptype = puzzle["sudokutype"]
        if ptype == "basic":
            dir_key = "sudoku"
        elif ptype == "xsudoku":
            dir_key = "xsudoku"
        elif ptype == "jigsaw":
            dir_key = "jigsaw"
        else:
            print(f"Warning: unknown type {ptype}, skipping {puzzle['name']}",
                  file=sys.stderr)
            continue

        slug = slugify(puzzle["name"])
        slug = deduplicate_slug(slug, slug_counts[dir_key])
        filename = f"{slug}.param"
        filepath = os.path.join(OUTPUT_DIRS[dir_key], filename)

        write_param(puzzle, filepath)
        counts[dir_key] += 1
        written.append((dir_key, filename, puzzle["name"], puzzle["technique"]))

    # Print summary
    total = sum(counts.values())
    print(f"Extracted {total} puzzles:")
    for dir_key, count in sorted(counts.items()):
        if count > 0:
            model = {
                "sudoku": "sudoku-domains.eprime",
                "xsudoku": "xsudoku-domains.eprime",
                "jigsaw": "jigsaw-domains.eprime",
            }[dir_key]
            print(f"  {count:3d} {dir_key} puzzles -> {OUTPUT_DIRS[dir_key]}/ (model: {model})")

    # Print by technique
    print()
    print("By technique:")
    techniques = {}
    for dir_key, filename, name, technique in written:
        techniques.setdefault(technique, []).append((dir_key, filename, name))
    for technique, items in techniques.items():
        print(f"  {technique} ({len(items)} puzzles)")
        for dir_key, filename, name in items:
            print(f"    [{dir_key}] {filename}: {name}")

    # Write README
    write_readme(written, techniques)


def write_readme(written, techniques):
    """Write a README.md for the sudokuwiki directory."""
    readme_path = os.path.join(OUTPUT_DIRS["sudoku"], "..", "README-sudokuwiki-domains.md")

    lines = []
    lines.append("# SudokuWiki domain puzzles")
    lines.append("")
    lines.append("Puzzles extracted from [sudokuwiki.org](https://www.sudokuwiki.org/),")
    lines.append("via the old Python demystify's `sudoku-wiki.py` test suite.")
    lines.append("")
    lines.append("These param files use the **domain** format: each cell specifies which")
    lines.append("values are candidates (rather than a single fixed value). This lets you")
    lines.append("test demystify on partially-reduced puzzles where specific solving")
    lines.append("techniques are expected to apply.")
    lines.append("")
    lines.append("## Models")
    lines.append("")
    lines.append("| Directory | Model file | Puzzle type |")
    lines.append("|-----------|-----------|-------------|")
    lines.append("| `sudokuwiki-domains/` | `sudoku-domains.eprime` | Standard sudoku |")
    lines.append("| `../xsudoku/sudokuwiki/` | `xsudoku-domains.eprime` | X-sudoku |")
    lines.append("| `../jigsaw/sudokuwiki/` | `jigsaw-domains.eprime` | Jigsaw sudoku |")
    lines.append("")
    lines.append("## Domain format")
    lines.append("")
    lines.append("The `allowed` parameter is a 9x9x9 binary matrix where")
    lines.append("`allowed[row][col][d-1]` is 1 if value d is a candidate for cell (row,col).")
    lines.append("")
    lines.append("Jigsaw puzzles additionally have a `regions` parameter: a 9x9 matrix")
    lines.append("giving the region number (1-9) for each cell.")
    lines.append("")
    lines.append("## Example usage")
    lines.append("")
    lines.append("```sh")
    lines.append("cargo run --release --bin demystify -- \\")
    lines.append("    --model eprime/sudoku-domains.eprime \\")
    lines.append("    --param eprime/sudoku/sudokuwiki-domains/x-wing-strategy-1a.param \\")
    lines.append("    --html --quick --trace > xwing.html")
    lines.append("```")
    lines.append("")
    lines.append("## Puzzles by technique")
    lines.append("")

    for technique, items in techniques.items():
        lines.append(f"### {technique} ({len(items)} puzzles)")
        lines.append("")
        for dir_key, filename, name in items:
            model = {
                "sudoku": "sudoku-domains",
                "xsudoku": "xsudoku-domains",
                "jigsaw": "jigsaw-domains",
            }[dir_key]
            lines.append(f"- `{filename}` ({model}): {name}")
        lines.append("")

    lines.append("## Regenerating")
    lines.append("")
    lines.append("```sh")
    lines.append("python3 scripts/extract-sudokuwiki.py")
    lines.append("```")
    lines.append("")

    with open(readme_path, "w") as f:
        f.write("\n".join(lines))
    print(f"\nWrote README to {readme_path}")


if __name__ == "__main__":
    main()
