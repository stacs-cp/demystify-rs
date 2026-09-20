#!/usr/bin/env python3
"""Export, validate and explore a reproducible multi-game solve-graph corpus.

The SQLite file indexes self-contained graph archives. No story classification
is performed here; incomplete searches remain distinguishable and resumable.
"""

import argparse
import concurrent.futures
import datetime
import hashlib
import json
import pathlib
import sqlite3
import subprocess
import threading
import time

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
BINARY = REPO / "target/release/demystify-solvetree"
RUN_FIELDS = [
    "status", "started_utc", "finished_utc", "seconds", "settings_json",
    "archive_path", "archive_sha256", "archive_bytes", "viewer_path",
    "active_nodes", "retained_nodes", "proofs", "difficulty_max", "fixed_point",
    "summary_json", "log_path", "error",
]


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


class Batch:
    def __init__(self, args):
        self.args = args
        self.root = args.corpus.resolve()
        self.manifest = json.loads((self.root / "manifest.json").read_text())
        self.phase = f"refined-{args.refine}" if args.refine else "baseline"
        self.subdir = self.phase if args.refine else ""
        self.lock = threading.Lock()
        for folder in ["graphs", "viewers", "logs"]:
            (self.root / folder / self.subdir).mkdir(parents=True, exist_ok=True)
        self.items = [
            item for item in self.manifest["instances"]
            if args.ids is None or item["id"] in args.ids
        ]
        unknown = set(args.ids or []) - {i["id"] for i in self.items}
        if unknown:
            raise ValueError(f"Unknown instance IDs: {sorted(unknown)}")
        self.initialize_database()

    def db(self):
        connection = sqlite3.connect(self.root / "corpus.sqlite", timeout=60)
        connection.execute("PRAGMA foreign_keys=ON")
        return connection

    def initialize_database(self):
        with self.db() as db:
            db.executescript("""
                CREATE TABLE IF NOT EXISTS games(
                    game TEXT PRIMARY KEY, inventory_json TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS puzzles(
                    id TEXT PRIMARY KEY, game TEXT NOT NULL REFERENCES games(game),
                    variant TEXT, source_pack TEXT, source_tier INTEGER,
                    board_area INTEGER, input_path TEXT, input_sha256 TEXT,
                    source_json TEXT, validation_status TEXT, validation_json TEXT,
                    parsed_path TEXT, parsed_sha256 TEXT);
                CREATE TABLE IF NOT EXISTS runs(
                    puzzle_id TEXT REFERENCES puzzles(id), phase TEXT, status TEXT,
                    started_utc TEXT, finished_utc TEXT, seconds REAL,
                    settings_json TEXT, archive_path TEXT, archive_sha256 TEXT,
                    archive_bytes INTEGER, viewer_path TEXT, active_nodes INTEGER,
                    retained_nodes INTEGER, proofs INTEGER, difficulty_max INTEGER,
                    fixed_point INTEGER, summary_json TEXT, log_path TEXT, error TEXT,
                    PRIMARY KEY(puzzle_id,phase));
                CREATE INDEX IF NOT EXISTS puzzle_game_tier
                    ON puzzles(game,source_tier);
                CREATE VIEW IF NOT EXISTS completed_graphs AS
                    SELECT p.id,p.game,p.variant,p.source_pack,p.source_tier,
                           p.board_area,r.*
                    FROM puzzles p JOIN runs r ON r.puzzle_id=p.id
                    WHERE r.status='complete';
                CREATE TABLE IF NOT EXISTS attempts AS SELECT * FROM runs WHERE 0;
                CREATE UNIQUE INDEX IF NOT EXISTS attempt_identity
                    ON attempts(puzzle_id,phase,started_utc);
                INSERT OR IGNORE INTO attempts
                    SELECT * FROM runs WHERE status<>'running';
            """)
            for game, inventory in self.manifest["inventory"].items():
                db.execute(
                    "INSERT INTO games VALUES (?,?) ON CONFLICT(game) "
                    "DO UPDATE SET inventory_json=excluded.inventory_json",
                    (game, json.dumps(inventory)),
                )
            for item in self.manifest["instances"]:
                db.execute(
                    "INSERT INTO puzzles(id,game,variant,source_pack,source_tier,"
                    "board_area,input_path,input_sha256,source_json) "
                    "VALUES (?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING",
                    (item["id"], item["game"], item["variant"], item["source_pack"],
                     item["source_tier"], item["board_area"], item["input"],
                     item["input_sha256"], json.dumps(item)),
                )

    def export(self):
        # Model bundles are shared by instances of each game, so export serially.
        for item in self.items:
            identifier = item["id"]
            parsed = self.root / "parsed" / f"{identifier}.json"
            validation_path = self.root / "validation" / f"{identifier}.json"
            if not (parsed.exists() and validation_path.exists()):
                log = self.root / "logs" / f"{identifier}.export.log"
                with log.open("a") as stream:
                    try:
                        result = subprocess.run(
                            ["node", str(HERE / "export.mjs"),
                             self.manifest["apps_root"], str(self.root), identifier],
                            stdout=stream, stderr=subprocess.STDOUT, timeout=120,
                        )
                    except subprocess.TimeoutExpired:
                        result = None
                if result is None or result.returncode:
                    with self.db() as db:
                        db.execute(
                            "UPDATE puzzles SET validation_status=?,validation_json=? "
                            "WHERE id=?",
                            ("export_failed", json.dumps({
                                "error": "Export failed or exceeded 120s",
                                "log": str(log.relative_to(self.root)),
                            }), identifier),
                        )
                    print(identifier, "EXPORT FAILED", flush=True)
                    continue
            validation = json.loads(validation_path.read_text())
            with self.db() as db:
                db.execute(
                    "UPDATE puzzles SET validation_status=?,validation_json=?,"
                    "parsed_path=?,parsed_sha256=? WHERE id=?",
                    (validation["status"], json.dumps(validation),
                     str(parsed.relative_to(self.root)), sha(parsed), identifier),
                )
            print(identifier, "validated", validation["status"], flush=True)

    def record(self, identifier, result):
        with self.lock:
            with self.db() as db:
                columns = ["puzzle_id", "phase"] + RUN_FIELDS
                values = [identifier, self.phase] + [result.get(k) for k in RUN_FIELDS]
                db.execute(
                    "INSERT INTO runs(" + ",".join(columns) + ") "
                    "VALUES (" + ",".join("?" for _ in columns) + ") "
                    "ON CONFLICT(puzzle_id,phase) DO UPDATE SET " +
                    ",".join(field + "=excluded." + field for field in RUN_FIELDS), values,
                )
                if result["status"] != "running":
                    db.execute(
                        "INSERT OR REPLACE INTO attempts SELECT * FROM runs "
                        "WHERE puzzle_id=? AND phase=?", (identifier, self.phase),
                    )
            print(identifier, self.phase, result["status"],
                  f"{result.get('seconds', 0):.2f}s", flush=True)

    def command(self, item, output):
        if output.exists():
            command = [str(BINARY), "resume", "--input", str(output)]
        elif self.args.refine:
            baseline = self.root / "graphs" / f"{item['id']}.graph.json"
            if not baseline.exists():
                raise ValueError("No baseline archive to refine")
            command = [str(BINARY), "resume", "--input", str(baseline),
                       "--out", str(output), "--repeats", str(self.args.refine)]
        else:
            command = [str(BINARY), "build", "--load-parsed",
                       str(self.root / "parsed" / f"{item['id']}.json"),
                       "--out", str(output), "--only-assign",
                       "--repeats", str(self.args.repeats), "--strategy", "dynamic",
                       "--conflict-limit", "1000"]
            if item["game"] == "bloomsweeper":
                command += ["--pin-assignment",
                            str(self.root / "inputs" / f"{item['id']}.pins.json")]
        return command + ["--threads", "2", "--checkpoint-seconds", "10"]

    def inspect(self, output, result):
        inspected = subprocess.run(
            [str(BINARY), "inspect", "--input", str(output)],
            check=True, capture_output=True, text=True,
        )
        summary = json.loads(inspected.stdout)
        graph = json.loads(output.read_text())
        seen = {graph["root"]}
        queue = list(seen)
        peak = 0
        for node in queue:
            for edge in graph["nodes"][node]["edges"]:
                if edge["active"]:
                    peak = max(peak, len(graph["proofs"][edge["proof"]]["constraints"]))
                    if edge["target"] not in seen:
                        seen.add(edge["target"])
                        queue.append(edge["target"])
        result.update({
            "archive_path": str(output.relative_to(self.root)),
            "archive_sha256": sha(output), "archive_bytes": output.stat().st_size,
            "summary_json": json.dumps(summary),
            "active_nodes": summary["active_nodes"],
            "retained_nodes": summary["retained_nodes"], "proofs": summary["proofs"],
            "difficulty_max": peak, "fixed_point": int(summary["fixed_point"]),
            "status": "checkpoint",
        })
        # Record the actual saved solver settings too, particularly on resume.
        settings = json.loads(result["settings_json"])
        settings["saved_solver_config"] = graph["solver_config"]
        settings["saved_searches"] = graph["searches"]
        result["settings_json"] = json.dumps(settings)
        if summary["fixed_point"]:
            if summary["underdetermined_leaves"]:
                result["status"] = "underdetermined"
            elif summary["search_incomplete_leaves"]:
                result["status"] = "search_incomplete"
            else:
                result["status"] = "complete"
        viewer = self.root / "viewers" / self.subdir / output.name.replace(".graph.json", ".html")
        subprocess.run(
            [str(BINARY), "render", "--input", str(output), "--out", str(viewer)],
            check=True, capture_output=True,
        )
        result["viewer_path"] = str(viewer.relative_to(self.root))

    def job(self, item):
        identifier = item["id"]
        output = self.root / "graphs" / self.subdir / f"{identifier}.graph.json"
        log = self.root / "logs" / self.subdir / f"{identifier}.run.log"
        start = time.monotonic()
        with self.db() as db:
            validation = db.execute(
                "SELECT validation_status FROM puzzles WHERE id=?", (identifier,),
            ).fetchone()[0]
            previous = db.execute(
                "SELECT status FROM runs WHERE puzzle_id=? AND phase=?",
                (identifier, self.phase),
            ).fetchone()
        if previous and previous[0] in ["complete", "underdetermined"] and output.exists():
            return
        result = {
            "status": "running", "started_utc": now(),
            "log_path": str(log.relative_to(self.root)),
            "settings_json": json.dumps({
                "repeats": self.args.refine or self.args.repeats,
                "strategy": "dynamic", "conflict_limit": 1000, "only_assign": True,
                "threads": 2, "budget_seconds": self.args.seconds,
            }),
        }
        self.record(identifier, result)
        try:
            if validation not in ["unique", "multiple"]:
                raise ValueError(f"Input validation status: {validation}")
            command = self.command(item, output)
            result["settings_json"] = json.dumps({
                **json.loads(result["settings_json"]), "command": command,
            })
            with log.open("a") as stream:
                stream.write("\n" + now() + "\n" + json.dumps(command) + "\n")
                stream.flush()
                try:
                    process = subprocess.run(
                        command, stdout=stream, stderr=subprocess.STDOUT,
                        timeout=self.args.seconds,
                    )
                    if process.returncode:
                        result["error"] = f"Solver exited with {process.returncode}; see log"
                except subprocess.TimeoutExpired:
                    result["error"] = (
                        f"Budget of {self.args.seconds}s reached; resume saved checkpoint"
                    )
            if output.exists():
                self.inspect(output, result)
            else:
                result["status"] = "failed"
        except Exception as error:
            result["status"] = "failed"
            result["error"] = str(error)
        result["seconds"] = round(time.monotonic() - start, 3)
        result["finished_utc"] = now()
        self.record(identifier, result)

    def run(self):
        self.export()
        if self.args.export_only:
            return
        with concurrent.futures.ThreadPoolExecutor(max_workers=self.args.workers) as pool:
            list(pool.map(self.job, self.items))
        with self.db() as db:
            print("RESULTS", db.execute(
                "SELECT phase,status,count(*) FROM runs GROUP BY phase,status"
            ).fetchall(), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", type=pathlib.Path, required=True)
    parser.add_argument("--workers", type=int, default=2)
    parser.add_argument("--seconds", type=int, default=180)
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--refine", type=int)
    parser.add_argument("--ids", nargs="+")
    parser.add_argument("--export-only", action="store_true")
    args = parser.parse_args()
    if min(args.workers, args.seconds, args.repeats) < 1 or (
        args.refine is not None and args.refine < 1
    ):
        parser.error("workers, seconds and MUS repeats must be positive")
    Batch(args).run()


if __name__ == "__main__":
    main()
