#!/usr/bin/env python3
"""Run one durable capped pass, refreshing cross-corpus statistics after each chunk."""

import argparse
from collections import defaultdict, deque
import concurrent.futures
import fcntl
import json
import os
from pathlib import Path
import random
import shutil
import sqlite3
import subprocess
import sys
import threading
import time

import overall
import run


def interleave(items, seed):
    groups = defaultdict(list)
    for item in items:
        groups[item['game']].append(item)
    queues = []
    for game, values in sorted(groups.items()):
        values.sort(key=lambda i: i['id'])
        random.Random(f'{seed}:{game}').shuffle(values)
        queues.append(deque(values))
    ordered = []
    while any(queues):
        for queue in queues:
            if queue:
                ordered.append(queue.popleft())
    return ordered


def remaining(items, rows):
    # A checkpoint has already received its first-pass budget. Restarting the
    # sweep continues the queue; it does not accidentally grant every timeout
    # another budget. Interrupted running jobs are recoverable on restart.
    attempted = {r['puzzle_id'] for r in rows if r['status'] != 'running'}
    return [i for i in items if i['id'] not in attempted]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--corpus', type=Path, required=True)
    parser.add_argument('--include-corpus', type=Path, action='append', default=[])
    parser.add_argument('--workers', type=int, default=4)
    parser.add_argument('--seconds', type=int, default=60)
    parser.add_argument('--chunk-size', type=int, default=40)
    parser.add_argument('--seed', type=int, default=20260913)
    parser.add_argument('--min-free-gib', type=float, default=10)
    args = parser.parse_args()
    if min(args.workers, args.seconds, args.chunk_size) < 1 or args.min_free_gib < 0:
        parser.error('workers, seconds and chunk-size must be positive; min-free-gib cannot be negative')
    root = args.corpus.resolve()
    # Holding this file lock for the entire pass prevents two sweep processes
    # from independently resuming and replacing the same graph archives.
    with (root / 'sweep.lock').open('a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            parser.error('A sweep already holds this corpus lock')
        execute(args, root)


def execute(args, root):
    args.refine, args.ids, args.export_only, args.repeats = None, None, False, 5
    batch = run.Batch(args)
    inventory = json.loads((root / 'inventory.json').read_text())
    roots = [p.resolve() for p in args.include_corpus] + [root]
    start = time.monotonic()
    progress_lock = threading.Lock()
    state = {'status': 'running', 'stage': 'starting', 'pid': os.getpid(), 'started_utc': run.now(),
             'workers': args.workers, 'solver_threads_per_job': 2, 'budget_seconds': args.seconds,
             'instances_in_this_pass': len(batch.items), 'current_inventory_entries': len(inventory['entries']),
             'included_corpora': [str(r) for r in roots], 'chunk_size': args.chunk_size, 'errors': []}

    def progress(**updates):
        with progress_lock:
            state.update(updates, updated_utc=run.now(), elapsed_seconds=round(time.monotonic()-start, 1))
            with batch.db() as db:
                state['runs'] = dict(db.execute("SELECT status,count(*) FROM runs WHERE phase='baseline' GROUP BY status"))
            state['pending'] = state['instances_in_this_pass'] - sum(state['runs'].values())
            state['free_gib'] = round(shutil.disk_usage(root).free / 2**30, 2)
            overall.save(root / 'progress.json', json.dumps(state, indent=2) + '\n')

    original_record = batch.record
    def record(identifier, result):
        original_record(identifier, result)
        progress()
    batch.record = record

    def report():
        overall.write(inventory, roots, root / 'overall')

    def analyse_ids(ids):
        if not ids:
            return
        progress(stage='analysing')
        with (root / 'logs/analysis.log').open('a') as log:
            try:
                result = subprocess.run([sys.executable, '-B', str(run.HERE / 'analyse.py'), '--corpus', str(root),
                                         '--ids', *ids], stdout=log, stderr=subprocess.STDOUT, timeout=600)
                code = result.returncode
            except subprocess.TimeoutExpired:
                code = 'timeout after 600 seconds'
        if code:
            state['errors'].append({'stage': 'analysis', 'ids': ids, 'returncode': code})

    try:
        # Keep native and WASM builds fixed even if the working checkout is
        # rebuilt while this several-hour pass is running.
        runtime = root / 'sources/runtime'
        runtime.mkdir(parents=True, exist_ok=True)
        binary = runtime / 'demystify-solvetree'
        if not binary.exists():
            shutil.copy2(run.BINARY, binary)
        run.BINARY = binary
        wasm = root / 'sources/wasm'
        wasm.mkdir(exist_ok=True)
        for name in ('demystify_wasm.js', 'demystify_wasm_bg.wasm', 'package.json'):
            if not (wasm / name).exists():
                shutil.copy2(run.REPO / 'demystify-wasm/pkg' / name, wasm / name)
        os.environ['DEMYSTIFY_CORPUS_WASM_DIR'] = str(wasm)
        state['native_binary_sha256'] = run.sha(binary)
        state['wasm_sha256'] = run.sha(wasm / 'demystify_wasm_bg.wasm')
        runner = root / 'sources/runner-at-start'
        runner.mkdir(exist_ok=True)
        for source in run.HERE.iterdir():
            if source.is_file() and not (runner / source.name).exists():
                shutil.copy2(source, runner / source.name)
        progress()
        report()
        with batch.db() as db:
            db.row_factory = sqlite3.Row
            pending = remaining(batch.items, db.execute("SELECT puzzle_id,status FROM runs WHERE phase='baseline'").fetchall())
            has_story = db.execute("SELECT 1 FROM sqlite_master WHERE name='story_runs'").fetchone()
            if has_story:
                catchup = [r[0] for r in db.execute('''SELECT r.puzzle_id FROM runs r LEFT JOIN story_runs s
                    ON r.puzzle_id=s.puzzle_id AND r.phase=s.phase
                    WHERE r.phase='baseline' AND r.status<>'running' AND
                    (s.puzzle_id IS NULL OR s.archive_sha256 IS NOT r.archive_sha256 OR s.status='error')''')]
            else:
                catchup = [r[0] for r in db.execute("SELECT puzzle_id FROM runs WHERE phase='baseline' AND status<>'running'")]
        for offset in range(0, len(catchup), args.chunk_size):
            analyse_ids(catchup[offset:offset+args.chunk_size])
        if catchup:
            report()
        queue = interleave(pending, args.seed)
        for offset in range(0, len(queue), args.chunk_size):
            if shutil.disk_usage(root).free < args.min_free_gib * 2**30:
                progress(status='paused_low_disk', stage='paused')
                report()
                return
            batch.items = queue[offset:offset+args.chunk_size]
            ids = [i['id'] for i in batch.items]
            progress(stage='exporting', chunk_ids=ids)
            batch.export()
            progress(stage='solving')
            with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
                list(pool.map(batch.job, batch.items))
            # Isolate analysis memory from the long-running supervisor, and
            # analyse only the newly generated chunk instead of all past runs.
            analyse_ids(ids)
            progress(stage='summarising')
            report()
        progress(stage='cataloguing')
        with (root / 'logs/catalog.log').open('a') as log:
            result = subprocess.run([sys.executable, '-B', str(run.HERE / 'catalog.py'), '--corpus', str(root)],
                                     stdout=log, stderr=subprocess.STDOUT)
        if result.returncode:
            state['errors'].append({'stage': 'catalogue', 'returncode': result.returncode})
        report()
        progress(status='complete_with_errors' if state['errors'] else 'complete', stage='finished', finished_utc=run.now())
    except BaseException as error:
        progress(status='failed', error=str(error))
        raise


if __name__ == '__main__':
    main()
