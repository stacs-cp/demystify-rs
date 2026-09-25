#!/usr/bin/env python3
"""Verify a finished batch and write its portable Markdown catalog and provenance.

This only inventories saved runs; it does not classify puzzle stories.
"""

import argparse
import collections
import datetime
import hashlib
import json
import pathlib
import shutil
import sqlite3
import statistics
import subprocess

from jsonio import read_json


HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def git(directory, *args):
    return subprocess.check_output(
        ["git", "-C", str(directory), *args], text=True
    ).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", type=pathlib.Path, required=True)
    args = parser.parse_args()
    root = args.corpus.resolve()
    manifest = json.loads((root / "manifest.json").read_text())
    apps = pathlib.Path(manifest["apps_root"])
    with sqlite3.connect(root / "corpus.sqlite") as db:
        db.row_factory = sqlite3.Row
        assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        assert not db.execute("PRAGMA foreign_key_check").fetchall()
        puzzles = db.execute("SELECT * FROM puzzles ORDER BY game,id").fetchall()
        runs = db.execute("SELECT * FROM runs ORDER BY phase,puzzle_id").fetchall()
        attempts = db.execute("SELECT * FROM attempts ORDER BY started_utc").fetchall()

    assert len(puzzles) == len(manifest["instances"])
    assert len(runs) >= len(puzzles), "Some instances have not been run"
    assert all(r["status"] != "running" for r in runs), "Batch is still running"
    verified = 0
    for row in puzzles:
        for prefix in ["input", "parsed"]:
            if prefix == 'parsed' and not row['parsed_path'] and row['validation_status'] == 'export_failed':
                continue
            assert row[f"{prefix}_path"], f"Missing {prefix}: {row['id']}"
            path = root / row[f"{prefix}_path"]
            assert sha(path) == row[f"{prefix}_sha256"], path
    for row in runs:
        if not row["archive_path"]:
            continue
        path = root / row["archive_path"]
        assert sha(path) == row["archive_sha256"], path
        graph = read_json(path)
        assert graph["format"] == "demystify-solve-graph", path
        assert bool(graph["fixed_point"]) == bool(row["fixed_point"]), path
        summary = json.loads(row["summary_json"])
        if row["status"] == "complete":
            assert graph["fixed_point"], path
            assert summary["solved_leaves"] > 0, path
            assert summary["underdetermined_leaves"] == 0, path
            assert summary["search_incomplete_leaves"] == 0, path
            assert all(not node["needs_expansion"] for node in graph["nodes"]), path
        assert not row['viewer_path'], path
        verified += 1

    # Save the actual encoder inputs, not just a revision of a possibly dirty
    # app checkout. Bundles alone reference a local WASM URL; keep the WASM too.
    sources = root / "sources"
    source_paths = set()
    for inputs in (sources / "bundles").glob("*.inputs.json"):
        for name in json.loads(inputs.read_text()):
            if name.startswith(("corpus:", "empty-browser:")):
                continue
            path = (REPO / name).resolve()
            if path.is_file() and path.is_relative_to(apps):
                source_paths.add(path)
    records = []
    for path in sorted(source_paths):
        relative = path.relative_to(apps)
        destination = sources / "apps" / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, destination)
        records.append({"source": str(relative), "sha256": sha(path)})
    for name in ["demystify_wasm.js", "demystify_wasm_bg.wasm"]:
        destination = sources / "wasm" / name
        destination.parent.mkdir(exist_ok=True)
        if not destination.exists():
            shutil.copy2(REPO / "demystify-wasm/pkg" / name, destination)
    # Preserve the exact scripts as well as keeping maintained copies in Git.
    for path in HERE.iterdir():
        if path.is_file():
            destination = sources / "runner" / path.name
            destination.parent.mkdir(exist_ok=True)
            shutil.copy2(path, destination)
    (sources / "demystify-working-tree.patch").write_text(git(REPO, "diff", "--binary") + "\n")
    # Graph support is not necessarily committed yet; include its source files.
    native_sources = [REPO / "demystify/src/bin/solvetree.rs"]
    native_sources += list((REPO / "demystify/src/problem/solvetree").glob("*"))
    for path in native_sources:
        if path.is_file():
            destination = sources / "native" / path.relative_to(REPO)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, destination)
    provenance = {
        "captured_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "demystify_revision": git(REPO, "rev-parse", "HEAD"),
        "apps_revision": git(apps, "rev-parse", "HEAD"),
        "demystify_status": git(REPO, "status", "--short"),
        "apps_status": git(apps, "status", "--short"),
        "native_binary_sha256": sha(sources / 'runtime/demystify-solvetree' if
                                    (sources / 'runtime/demystify-solvetree').exists() else
                                    REPO / "target/release/demystify-solvetree"),
        "wasm_sha256": sha(sources / "wasm/demystify_wasm_bg.wasm"),
        "encoder_sources": records,
        "verified_archives": verified,
    }
    (sources / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")

    counts = collections.Counter(row["status"] for row in runs)
    selected_games = sorted({row["game"] for row in puzzles})
    completed_times = [row["seconds"] for row in runs if row["status"] == "complete"]
    timing = (
        f"Completed runs: median {statistics.median(completed_times):.2f}s, "
        f"maximum {max(completed_times):.2f}s; "
        f"{sum(t < 10 for t in completed_times)} of {len(completed_times)} finished "
        "in under 10 seconds. "
        if completed_times else ""
    )
    timing += (
        f"All {len(attempts)} attempts used a combined "
        f"{sum(row['seconds'] or 0 for row in attempts) / 60:.1f} puzzle-minutes "
        "(including unfinished work; concurrent puzzles overlap). "
        "The attempts table preserves the time spent before each resume."
    )
    lines = [
        "# Multi-game solving corpus",
        "",
        f"{len(puzzles)} instances from {len(selected_games)} games; "
        + ", ".join(f"{n} {status}" for status, n in sorted(counts.items())) + ".",
        "",
        "[SQLite database](corpus.sqlite) · [Selection and input provenance](manifest.json) · "
        "[Encoder/build provenance](sources/provenance.json) · "
        "[Runner instructions](sources/runner/README.md)",
        "",
        "The database indexes the JSON archives alongside it: keep this directory together. "
        "Each graph embeds its puzzle model, constraint descriptions, proofs, states, edges, "
        "search settings and proof propagation progress. Render an HTML viewer on demand.",
        "",
        manifest.get('selection', 'Selection is recorded in manifest.json.') + ' These are exploratory, '
        "deliberately selected examples, not a representative random sample. Source MUS tiers "
        "use each game's encoding and are not a calibrated scale across games. Unlabelled packs "
        "retain their names without an invented numeric difficulty. Board area is a size proxy "
        "(a bounding rectangle for some hex boards).",
        "",
        "`complete` means the current easiest-move graph and discovered-proof propagation "
        "reached a fixed point with solved leaves. It is relative to the searched MUS evidence, "
        "not a guarantee of globally smallest explanations. `checkpoint` is unfinished, saved "
        "work which can be resumed. `underdetermined` means multiple solutions leave unresolved "
        "cells. Exclude these statuses "
        "when comparing completed routes. The `completed_graphs` SQL view does this.",
        "",
        "Default search: five dynamic MUS repeats, assignments only, initial conflict limit "
        "1,000 (the solver may raise it), two solver threads, two concurrent puzzles. Runs "
        "are capped at the recorded wall-clock budget and save checkpoints between work units. "
        "Runtime includes archive inspection. No story classifications "
        "have been assigned.",
        "",
        timing,
        "",
        "## Coverage",
        "",
        "| Game | Instances | Source tiers / packs | Complete runs | Unfinished / ambiguous runs |",
        "| --- | ---: | --- | ---: | ---: |",
    ]
    if (root / 'statistics/README.md').exists():
        lines[5:5] = ['[Story statistics and witness routes](statistics/README.md)', '']
    if (root / 'EXPANSION.md').exists():
        lines[5:5] = ['[Expansion results and all-puzzles runtime estimate](EXPANSION.md)', '']
    if (root / 'overall/README.md').exists():
        lines[5:5] = ['[Overall per-game distributions](overall/README.md) · [Batch progress](progress.json)', '']
    for game in selected_games:
        ps = [p for p in puzzles if p["game"] == game]
        ids = {p["id"] for p in ps}
        rs = [r for r in runs if r["puzzle_id"] in ids]
        tiers = sorted({p["source_tier"] for p in ps if p["source_tier"] is not None})
        labels = ", ".join(map(str, tiers)) if tiers else ", ".join(sorted({p["source_pack"] for p in ps}))
        complete = sum(r["status"] == "complete" for r in rs)
        lines.append(f"| {game} | {len(ps)} | {labels} | {complete} | {len(rs)-complete} |")
    lines += [
        "",
        "## Saved runs",
        "",
        "| Instance | Pass | Status | Seconds | Active states | Peak MUS | Saved files |",
        "| --- | --- | --- | ---: | ---: | ---: | --- |",
    ]
    for row in runs:
        files = []
        for column, label in [("archive_path", "graph"), ("viewer_path", "viewer"), ("log_path", "log")]:
            if row[column]:
                files.append(f"[{label}]({row[column]})")
        duration = f"{row['seconds']:.2f}" if row["seconds"] is not None else "—"
        lines.append(
            f"| {row['puzzle_id']} | {row['phase']} | {row['status']} | {duration} | "
            f"{row['active_nodes'] if row['active_nodes'] is not None else '—'} | "
            f"{row['difficulty_max'] if row['difficulty_max'] is not None else '—'} | "
            + " · ".join(files) + " |"
        )
    (root / "README.md").write_text("\n".join(lines) + "\n")
    print(f"Verified {verified} graph archives; catalog: {root / 'README.md'}")


if __name__ == "__main__":
    main()
