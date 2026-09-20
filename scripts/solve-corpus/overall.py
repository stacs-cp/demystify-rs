#!/usr/bin/env python3
"""Combine current puzzle instances and per-puzzle story distributions across corpora."""

import argparse
from collections import Counter, defaultdict
import datetime
import json
from pathlib import Path
import sqlite3

from expand import fingerprint, size_band
from story import VERSION


METRICS = {
    'moves_expected': 'Expected move count under uniform cheapest-successor choices.',
    'peak_mus': 'Largest MUS on the saved cheapest-route graph; scales differ between encodings.',
    'chain_min': 'Smallest longest release chain over complete routes.',
    'chain_max': 'Largest longest release chain over complete routes.',
    'choke_step_fraction': 'Expected width-one encounters / expected moves.',
    'independent_step_fraction': 'Expected encounters with multiple locally commuting groups / expected moves; not global independence.',
    'cheapest_variable_fraction': 'Visit-weighted cheapest/unresolved variable fraction at nonterminal states.',
    'peak_hard_min': 'Fewest moves using the puzzle’s peak MUS over complete routes.',
    'peak_hard_max': 'Most moves using the puzzle’s peak MUS over complete routes.',
    'peak_hard_gap': 'Difference between most and fewest peak-MUS moves.',
    'peak_streak_max': 'Largest consecutive run of peak-MUS moves on any route.',
    'peak_choke_min': 'Fewest width-one peak-MUS encounters over complete routes.',
    'peak_choke_max': 'Most width-one peak-MUS encounters over complete routes.',
    'peak_new_easy_expected': 'Expected total newly easy literal events after peak-MUS moves (easy means below that peak).',
    'peak_tail_expected': 'Expected easy moves after the last peak-MUS move, conditional on a route having one.',
    'repeated_profile_fraction': 'Expected repeated constraint-family profile uses / expected moves; a metadata proxy.',
}


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.tmp')
    temporary.write_text(value)
    temporary.replace(path)


def distribution(values):
    values = sorted(v for v in values if v is not None)
    if not values:
        return {'n': 0}
    def q(p):
        at = (len(values) - 1) * p
        lo = int(at)
        return values[lo] + (values[min(lo + 1, len(values) - 1)] - values[lo]) * (at - lo)
    return {'n': len(values), 'min': values[0], 'p10': q(.1), 'p25': q(.25),
            'median': q(.5), 'p75': q(.75), 'p90': q(.9), 'max': values[-1]}


def features(db, row):
    key = (row['puzzle_id'], 'baseline')
    values = {k: None for k in METRICS}
    values.update(peak_mus=row['peak_max'], chain_min=row['chain_min'], chain_max=row['chain_max'])
    flow = db.execute('SELECT * FROM story_flow WHERE puzzle_id=? AND phase=?', key).fetchone()
    if not flow:
        return values
    moves = flow['expected_moves']
    values['moves_expected'] = moves
    def fraction(value):
        return value / moves if value is not None and moves else None
    values['repeated_profile_fraction'] = fraction(flow['expected_repeated_profiles'])
    weighted = db.execute('''SELECT
        sum(CASE WHEN i.width=1 THEN a.visit_probability ELSE 0 END),
        sum(CASE WHEN i.local_groups>1 THEN a.visit_probability ELSE 0 END),
        sum(a.visit_probability*a.variable_fraction),
        sum(CASE WHEN a.variable_fraction IS NOT NULL THEN a.visit_probability ELSE 0 END)
        FROM story_availability a JOIN story_independence i
        ON a.puzzle_id=i.puzzle_id AND a.phase=i.phase AND a.state=i.state
        WHERE a.puzzle_id=? AND a.phase=? AND i.width>0''', key).fetchone()
    values['choke_step_fraction'] = fraction(weighted[0])
    values['independent_step_fraction'] = fraction(weighted[1])
    values['cheapest_variable_fraction'] = weighted[2] / weighted[3] if weighted[3] else None
    hard = db.execute('SELECT * FROM story_thresholds WHERE puzzle_id=? AND phase=? AND threshold=?',
                      key + (row['peak_max'],)).fetchone()
    if hard:
        values.update(peak_hard_min=hard['hard_min'], peak_hard_max=hard['hard_max'],
                      peak_hard_gap=hard['hard_gap'], peak_streak_max=hard['streak_max'],
                      peak_choke_min=hard['choke_min'], peak_choke_max=hard['choke_max'])
    pace = db.execute('SELECT * FROM story_pacing WHERE puzzle_id=? AND phase=? AND threshold=?',
                      key + (row['peak_max'],)).fetchone()
    if pace:
        values.update(peak_new_easy_expected=pace['expected_new_easy'], peak_tail_expected=pace['expected_tail_given_hard'])
    return values


def collect(inventory, roots):
    # Keep source entries as the population, but attach observations by frozen
    # content. Renamed levels can reuse evidence; changed inputs cannot.
    observed = {}
    for root in roots:
        manifest = json.loads((root / 'manifest.json').read_text())
        if not (root / 'corpus.sqlite').exists():
            continue
        hashes = {i['id']: i.get('content_sha256') or fingerprint(json.loads((root / i['input']).read_text()))
                  for i in manifest['instances']}
        with sqlite3.connect(root / 'corpus.sqlite', timeout=60) as db:
            db.row_factory = sqlite3.Row
            tables = {r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type IN ('table','view')")}
            for run in db.execute("SELECT * FROM runs WHERE phase='baseline'").fetchall():
                content = hashes[run['puzzle_id']]
                item = {'corpus': str(root), 'puzzle_id': run['puzzle_id'], 'status': run['status'],
                        'seconds': run['seconds'], 'active_states': run['active_nodes'],
                        'archive_sha256': run['archive_sha256'], 'archive_path': run['archive_path'],
                        'analysis_status': 'pending', 'metrics': {}}
                if 'current_story_statistics' in tables and run['status'] == 'complete':
                    row = db.execute('SELECT * FROM current_story_statistics WHERE puzzle_id=? AND phase=? AND version=?',
                                     (run['puzzle_id'], 'baseline', VERSION)).fetchone()
                    if row:
                        item.update(analysis_status=row['status'], report_path=row['report_path'],
                                    statistics_sha256=row['statistics_sha256'], metrics=features(db, row))
                    else:
                        error = db.execute('SELECT status FROM story_runs WHERE puzzle_id=? AND phase=?',
                                           (run['puzzle_id'], 'baseline')).fetchone()
                        if error:
                            item['analysis_status'] = error[0]
                rank = (item['status'] == 'complete', bool(item['metrics']), item['status'] != 'running')
                old = observed.get(content)
                if old is None or rank > old[0]:
                    observed[content] = (rank, item)
    result = []
    for source in inventory['entries']:
        entry = {k: source[k] for k in ('id', 'game', 'variant', 'source_pack', 'source_tier', 'board_area', 'content_sha256')}
        entry['size_band'] = size_band(entry['board_area'])
        entry.update(observed.get(source['content_sha256'], (None, {'status': 'pending', 'analysis_status': 'pending', 'metrics': {}}))[1])
        result.append(entry)
    return result


def summarise(entries):
    grouped = defaultdict(list)
    for entry in entries:
        for dimension, key in [('game', [entry['game']]), ('game_size', [entry['game'], entry['size_band']]),
                               ('game_pack', [entry['game'], entry['source_pack']]),
                               ('game_tier', [entry['game'], entry['source_tier']])]:
            grouped[(dimension, json.dumps(key))].append(entry)
    groups = []
    for (dimension, key), values in sorted(grouped.items()):
        analysed = [v for v in values if v['status'] == 'complete' and v['metrics']]
        counts = dict(Counter(v['status'] for v in values))
        groups.append({'dimension': dimension, 'key': json.loads(key), 'population': len(values),
                       'statuses': counts, 'analysed': len(analysed),
                       'analysis_pending_or_error': counts.get('complete', 0) - len(analysed),
                       'metrics': {k: distribution(v['metrics'].get(k) for v in analysed) for k in METRICS}})
    return groups


def markdown(result):
    entries, groups = result['puzzles'], result['groups']
    counts = Counter(v['status'] for v in entries)
    games = [g for g in groups if g['dimension'] == 'game']
    lines = ['# Overall puzzle story statistics', '', f"Updated {result['computed_utc']}.", '',
             f"{len(entries):,} current source entries; " + ', '.join(f'{n:,} {s}' for s, n in sorted(counts.items())) + '.', '',
             '[Queryable SQLite index](overall.sqlite) · [Full distributions and puzzle rows](overall.json) · [Batch progress](../progress.json)', '',
             '**All story comparisons below are conditional on graph completion.** Slow, highly branching puzzles may be missing disproportionately. '
             'Pending and unfinished graphs are counted in coverage, never treated as zero-valued stories. '
             'Until coverage improves, these are hypotheses to investigate rather than population rankings.', '',
             'Each puzzle has equal weight in these distributions. Fractions use expected encounter counts divided by expected moves under '
             'uniform distinct cheapest-successor choices; they are not observations of human behaviour. '
             'MUS sizes and constraint-family profiles use each game’s encoding and are not calibrated across games.', '',
             '## Coverage', '',
             '| Game | Current | Complete | Analysed | Checkpoint | Nonunique | Other attempted | Pending/running |',
             '| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |']
    for g in games:
        c = g['statuses']
        other = sum(v for k, v in c.items() if k not in ('complete', 'checkpoint', 'underdetermined', 'pending', 'running'))
        lines.append(f"| {g['key'][0]} | {g['population']} | {c.get('complete',0)} | {g['analysed']} | "
                     f"{c.get('checkpoint',0)} | {c.get('underdetermined',0)} | {other} | {c.get('pending',0)+c.get('running',0)} |")
    def table(title, columns):
        lines.extend(['', '## ' + title, '', 'Medians across analysed puzzles; — means unavailable.', '',
                      '| Game | Analysed | ' + ' | '.join(label for _, label, _ in columns) + ' |',
                      '| --- | ---: | ' + ' | '.join('---:' for _ in columns) + ' |'])
        for g in games:
            cells = []
            for metric, _, percent in columns:
                value = g['metrics'][metric].get('median')
                cells.append('—' if value is None else f'{100*value:.1f}%' if percent else f'{value:.2f}')
            lines.append(f"| {g['key'][0]} | {g['analysed']} | " + ' | '.join(cells) + ' |')
    table('Route structure', [('chain_max', 'Longest release chain', False), ('choke_step_fraction', 'Choke steps', True),
                             ('independent_step_fraction', 'Local independent-group steps', True),
                             ('cheapest_variable_fraction', 'Cheapest/unresolved variables', True)])
    table('Hard moves and endings', [('peak_mus', 'Peak MUS', False), ('peak_hard_min', 'Fewest peak moves', False),
                                    ('peak_hard_max', 'Most peak moves', False), ('peak_hard_gap', 'Peak-count gap', False),
                                    ('peak_tail_expected', 'Easy tail after peak', False)])
    lines += ['', 'JSON and SQLite also contain sample counts, minima, quartiles, p10/p90 and maxima for every metric, '
              'split by game, board-size band, source pack and source tier. Missing metrics retain their own sample counts. '
              'Local commuting groups do not certify global independent subproblems. A release chain records enabling events, '
              'not individually necessary prerequisites.', '', '## Metric definitions', '']
    lines.extend(f'- `{key}`: {value}' for key, value in METRICS.items())
    return '\n'.join(lines) + '\n'


def write(inventory, roots, out):
    entries = collect(inventory, roots)
    result = {'computed_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'corpora': [str(r) for r in roots], 'metric_definitions': METRICS,
              'puzzles': entries, 'groups': summarise(entries)}
    out.mkdir(parents=True, exist_ok=True)
    database = out / 'overall.sqlite'
    # Replace the small derived index atomically. Generation databases stay separate.
    temporary = database.with_name(database.name + '.tmp')
    with sqlite3.connect(temporary) as db:
        db.executescript('DROP TABLE IF EXISTS puzzles; DROP TABLE IF EXISTS distributions; '
                         'DROP TABLE IF EXISTS metadata; '
                         'CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT); '
                         'CREATE TABLE puzzles(id TEXT PRIMARY KEY,game TEXT,variant TEXT,source_pack TEXT,source_tier INTEGER,'
                         'size_band TEXT,board_area INTEGER,status TEXT,analysis_status TEXT,corpus TEXT,puzzle_id TEXT,'
                         'report_path TEXT,content_sha256 TEXT,' + ','.join(k+' REAL' for k in METRICS) + ',record_json TEXT); '
                         'CREATE TABLE distributions(dimension TEXT,group_key TEXT,metric TEXT,n INTEGER,'
                         'minimum REAL,p10 REAL,p25 REAL,median REAL,p75 REAL,p90 REAL,maximum REAL,'
                         'PRIMARY KEY(dimension,group_key,metric));')
        for key in ('computed_utc', 'corpora', 'metric_definitions'):
            db.execute('INSERT INTO metadata VALUES (?,?)', (key, json.dumps(result[key])))
        keys = ('id','game','variant','source_pack','source_tier','size_band','board_area','status',
                'analysis_status','corpus','puzzle_id','report_path','content_sha256')
        for entry in entries:
            vals = tuple(entry.get(k) for k in keys) + tuple(entry['metrics'].get(k) for k in METRICS) + (json.dumps(entry),)
            db.execute('INSERT INTO puzzles VALUES (' + ','.join('?' for _ in vals) + ')', vals)
        for g in result['groups']:
            for metric, d in g['metrics'].items():
                vals = (g['dimension'], json.dumps(g['key']), metric, d['n']) + tuple(d.get(k) for k in ('min','p10','p25','median','p75','p90','max'))
                db.execute('INSERT INTO distributions VALUES (?,?,?,?,?,?,?,?,?,?,?)', vals)
        assert db.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
    temporary.replace(database)
    save(out / 'overall.json', json.dumps(result, indent=2) + '\n')
    save(out / 'README.md', markdown(result))
    print('OVERALL', dict(Counter(e['status'] for e in entries)), 'analysed', sum(bool(e['metrics']) for e in entries), flush=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inventory', type=Path, required=True)
    parser.add_argument('--corpus', type=Path, action='append', required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    write(json.loads(args.inventory.read_text()), [r.resolve() for r in args.corpus], args.out.resolve())


if __name__ == '__main__':
    main()
