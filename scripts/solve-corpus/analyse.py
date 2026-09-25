#!/usr/bin/env python3
"""Analyse a saved solve graph or index story statistics for a complete corpus."""

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import time

from story import VERSION, IncompleteGraph, analyse
from jsonio import read_bytes, read_json, write_text

HERE = Path(__file__).resolve().parent


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def save(path, contents):
    write_text(path, contents)


def report_path(path):
    path = Path(path)
    if path.suffix == '.zst':
        path = path.with_suffix('')
    return path.with_suffix('.md')


def span(row):
    return str(row['min']) if row['min'] == row['max'] else f"{row['min']}–{row['max']}"


def literal_names(graph):
    result = {}
    for text, literals in graph.get('puzzle', {}).get('invlitmap', {}).items():
        labels = []
        for lit in literals:
            varval = lit['varval']
            var = varval['var']
            index = '[' + ','.join(map(str, var['indices'])) + ']' if var['indices'] else ''
            labels.append(f"{var['name']}{index}{'=' if lit['equal'] else '!='}{varval['val']}")
        result[int(text)] = ' / '.join(labels)
    return result


def report(result, graph, title, viewer=None):
    names = literal_names(graph)
    def name(lit):
        return names.get(lit, str(lit)).replace('|', '\\|')
    chain = result['release']['chains']
    lines = [f'# {title}', '',
             'Only current cheapest-move routes are included. One step applies one MUS, '
             'including all of its explicit deductions and automatic closure. '
             'Alternative explanations with identical consequences do not inflate choice width.', '',
             f"**{result['state_routes']} distinct state routes**, {result['active_states']} active states; "
             f"{span(result['moves'])} moves; peak MUS {span(result['peak_mus'])}.", '']
    if viewer:
        lines += [f'[Open the puzzle graph viewer]({viewer}) — use the state numbers below to inspect a move.', '']
    if chain['status'] == 'complete':
        lines += [f"Longest release chain: **{span(chain)} moves** across routes. "
                  f"[Minimum witness](#{chain['min_route']}) · [Maximum witness](#{chain['max_route']}).", '']
    else:
        lines += [f"Release-chain extrema unavailable: {chain['reason']}. Other metrics are exact on this saved DAG.", '']
    lines += ['A release is a drop in the best discovered MUS size for a deduction. '
              'Its depth follows the move that first made the eventual cost available along that route, '
              'even if independent moves intervened. Initial availability has depth 1. '
              'These are path-specific enabling events, not certificates of individually necessary facts. '
              'Several earlier facts can jointly enable a proof; the recorded event is the last transition. '
              'Alternative explanations are combined by their best cost.', '',
              '## Repeated hard moves', '',
              'Each threshold has its own best and worst routes. The extrema for different thresholds '
              'need not be achieved by the same route. Counts and streaks each have their own witnesses.', '',
              '| MUS at least | Number of moves | Longest consecutive run | Fewest witness | Most witness |',
              '| ---: | ---: | ---: | --- | --- |']
    for row in result['hard_moves']:
        lines.append(f"| {row['size_at_least']} | {span(row)} | {span(row['longest_streak'])} | "
                     f"[{row['min_route']}](#{row['min_route']}) | [{row['max_route']}](#{row['max_route']}) |")
    lines += ['', '## Choke points', '',
              f"Hardest choke point per route: MUS **{span(result['chokes']['peak_mus'])}** "
              '(0 means a route has no choke point).', '',
              'Width counts distinct successor states. A width-one state has one resulting move, '
              'possibly justified by multiple MUSes or deducing several cells. '
              '“Unavoidable state” means every complete route visits that exact state. '
              'Route encounter bounds also capture unavoidable choke points at different states.', '',
              '| Maximum choice width | Encounters per route | Longest consecutive run |',
              '| ---: | ---: | ---: |']
    for row in result['chokes']['narrow_routes']:
        lines.append(f"| {row['width_at_most']} | {span(row['encounters'])} | {span(row['longest_streak'])} |")
    lines += ['', '| Choke MUS at least | Choke encounters per route |', '| ---: | ---: |']
    for row in result['chokes']['by_threshold']:
        lines.append(f"| {row['size_at_least']} | {span(row)} |")
    lines += ['', '| State | MUS size | Unavoidable state | Distinct deductions | Explanations |',
              '| ---: | ---: | --- | ---: | ---: |']
    for row in result['chokes']['states']:
        lines.append(f"| {row['state']} | {row['mus_size']} | {'yes' if row['unavoidable_state'] else 'no'} | "
                     f"{row['deduction_width']} | {row['explanation_width']} |")
    lines += ['', '## Releases and promotions', '']
    for key, label in [('cost_drop_moves', 'Moves reducing a remaining deduction’s MUS size'),
                       ('cheapest_release_moves', 'Moves enabling a new cheapest deduction at a smaller cost'),
                       ('queue_promotion_moves', 'Moves promoting already-available deductions to cheapest')]:
        lines.append(f"- {label}: **{span(result['release'][key])}** per route.")
    lines += flow_report(result)
    lines += ['', 'The JSON contains every extremum’s route reference, release events, proof IDs, '
              'and width/difficulty histogram. All results are relative to the discovered-proof '
              'fixed point; further MUS searches can change them.', '', '## Witness routes', '']
    for identifier, route in result['witnesses'].items():
        lines += [f'<a id="{identifier}"></a>', f'### {identifier}', '',
                  'Difficulty sequence: ' + (' → '.join(map(str, route['difficulty_sequence'])) or '(already solved)'), '',
                  'Longest release chain (step numbers): ' +
                  (' → '.join(str(i + 1) for i in route['release']['longest_chain_steps']) or '(none)'), '',
                  '| Step | State → state | MUS | Deductions | Release depth | Released after step(s) |',
                  '| ---: | --- | ---: | --- | ---: | --- |']
        for index, (step, event) in enumerate(zip(route['steps'], route['release']['events'])):
            parents = ', '.join(str(i + 1) for i in event['parents']) or 'available initially'
            deductions = ', '.join(name(lit) for lit in step['deduced'])
            lines.append(f"| {index + 1} | {step['state']} → {step['target']} | {step['mus_size']} | "
                         f"{deductions} | {event['depth']} | {parents} |")
        lines.append('')
    return '\n'.join(lines)


def flow_report(result):
    flow = result['flow']
    def optional_span(row):
        return span(row) if row.get('min') is not None else '—'
    lines = ['', '## Payoff and routine endings', '',
             'For threshold k, hard means MUS ≥ k and easy means MUS < k. Payoff bounds are '
             'over individual hard events on any complete cheapest route. Following easy work '
             'stops before the next hard move or at completion. Between-hard intervals require '
             'another hard move. A routine tail begins after the last hard move; routes with no '
             'hard move are a separate category in JSON. These are temporal stretches, not claims '
             'that all following progress was caused by that hard move.', '',
             '| k | Newly easy deductions | Following easy moves | Between-hard easy moves | Routine tail moves |',
             '| ---: | ---: | ---: | ---: | ---: |']
    for row in flow['thresholds']:
        lines.append(f"| {row['hard_threshold']} | {optional_span(row['newly_easy_literals'])} | "
                     f"{optional_span(row['following_easy_moves'])} | {optional_span(row['between_hard_easy_moves'])} | "
                     f"{optional_span(row['routine_tail_moves']['with_hard_move'])} |")
    lines += ['', 'Newly easy deductions cross the threshold through a cost drop. The JSON separately '
              'records deductions with no previously discovered proof and old easy work remaining. '
              'Direct deductions, automatic closure, and newly assigned primary puzzle variables are '
              'also separate. A primary variable is not necessarily one board cell.', '',
              '## Random tie-breaking baseline', '', flow['random_policy'], '',
              f"Expected moves: **{flow['expected_moves']:.3f}**. These are model predictions, not player observations.", '',
              '| k | Expected hard moves | Chance of a hard choke | Expected tail, given a hard move |',
              '| ---: | ---: | ---: | ---: |']
    for row in flow['thresholds']:
        tail = row['routine_tail_moves']['with_hard_move']['expected_conditional']
        lines.append(f"| {row['hard_threshold']} | {row['expected_hard_moves']:.3f} | "
                     f"{100 * row['probability_any_hard_choke']:.1f}% | {f'{tail:.3f}' if tail is not None else '—'} |")
    ind = flow['independence']
    lines += ['', '## Local order flexibility', '',
              f"**{ind['union_diamonds']}** two-step union diamonds, of which **{ind['stable_disjoint_pairs']}** "
              'have disjoint knowledge gains and preserve the move cost in both orders. Counts are '
              'over distinct states, not weighted by visitation probability.', '',
              f"Peak number of local groups per route: **{span(ind['peak_local_groups'])}**; "
              f"moves with multiple groups available: **{span(ind['multi_group_moves'])}**. "
              f"{ind['reconverging_branch_states']} of {ind['branch_states']} branching states have a common later state.", '',
              'Choices in different local groups commute pairwise at that state. Groups are connected '
              'components of the non-commutation relation; they are not a decomposition of all future '
              'work. The JSON records the first common later state and up to eight diamond examples '
              'per state, with an explicit truncation flag.', '',
              '## Repetition and discoverability proxies', '']
    profiles = flow['proof_profiles']
    if profiles['expected_distinct_profiles'] is not None:
        lines += [f"Expected distinct constraint-family profiles: **{profiles['expected_distinct_profiles']:.3f}**; "
                  f"expected repeated uses: **{profiles['expected_repeated_profile_uses']:.3f}**.", '']
    lines += [f"Constraint-family metadata: **{profiles['status']}**; primary-variable mapping: "
              f"**{flow['metadata']['variable_mapping']}**. Family multisets are coarse structural proxies, "
              'not identified human techniques. Names are retained exactly, including instance-specific names. '
              'The JSON contains per-profile usage and probabilities of ever encountering a profile, per-state easiest '
              'deduction density, and per-proof display targets. Bounding boxes are included only when '
              'all recorded targets can be read as coordinate pairs; flat cell indices are not treated '
              'as coordinates. Display targets are not necessarily the full logical scope of a proof.', '']
    return lines


SCHEMA = '''
CREATE TABLE IF NOT EXISTS story_runs(
    puzzle_id TEXT NOT NULL, phase TEXT NOT NULL, version INTEGER NOT NULL,
    archive_sha256 TEXT, status TEXT NOT NULL, computed_utc TEXT, seconds REAL,
    statistics_path TEXT, statistics_sha256 TEXT, report_path TEXT, error TEXT,
    route_count TEXT, moves_min INTEGER, moves_max INTEGER,
    peak_min INTEGER, peak_max INTEGER, chain_min INTEGER, chain_max INTEGER,
    PRIMARY KEY(puzzle_id,phase),
    FOREIGN KEY(puzzle_id,phase) REFERENCES runs(puzzle_id,phase));
CREATE TABLE IF NOT EXISTS story_thresholds(
    puzzle_id TEXT, phase TEXT, threshold INTEGER,
    hard_min INTEGER, hard_max INTEGER, hard_gap INTEGER,
    streak_min INTEGER, streak_max INTEGER, choke_min INTEGER, choke_max INTEGER,
    min_route TEXT, max_route TEXT,
    PRIMARY KEY(puzzle_id,phase,threshold),
    FOREIGN KEY(puzzle_id,phase) REFERENCES story_runs(puzzle_id,phase) ON DELETE CASCADE);
CREATE TABLE IF NOT EXISTS story_chokes(
    puzzle_id TEXT, phase TEXT, state INTEGER, mus_size INTEGER,
    unavoidable INTEGER, deduction_width INTEGER, batch_width INTEGER, explanation_width INTEGER,
    PRIMARY KEY(puzzle_id,phase,state),
    FOREIGN KEY(puzzle_id,phase) REFERENCES story_runs(puzzle_id,phase) ON DELETE CASCADE);
CREATE TABLE IF NOT EXISTS story_widths(
    puzzle_id TEXT, phase TEXT, width INTEGER, mus_size INTEGER, states INTEGER,
    PRIMARY KEY(puzzle_id,phase,width,mus_size),
    FOREIGN KEY(puzzle_id,phase) REFERENCES story_runs(puzzle_id,phase) ON DELETE CASCADE);
CREATE TABLE IF NOT EXISTS story_narrow(
    puzzle_id TEXT, phase TEXT, width INTEGER, encounters_min INTEGER, encounters_max INTEGER,
    streak_min INTEGER, streak_max INTEGER,
    PRIMARY KEY(puzzle_id,phase,width),
    FOREIGN KEY(puzzle_id,phase) REFERENCES story_runs(puzzle_id,phase) ON DELETE CASCADE);
CREATE TABLE IF NOT EXISTS story_flow(
    puzzle_id TEXT, phase TEXT, version INTEGER, random_policy TEXT,
    variable_mapping TEXT, profile_status TEXT,
    expected_moves REAL, expected_distinct_profiles REAL, expected_repeated_profiles REAL,
    union_diamonds INTEGER, stable_pairs INTEGER, groups_min INTEGER, groups_max INTEGER,
    multi_group_min INTEGER, multi_group_max INTEGER,
    PRIMARY KEY(puzzle_id,phase),
    FOREIGN KEY(puzzle_id,phase) REFERENCES story_runs(puzzle_id,phase) ON DELETE CASCADE);
CREATE TABLE IF NOT EXISTS story_pacing(
    puzzle_id TEXT, phase TEXT, threshold INTEGER,
    new_easy_min INTEGER, new_easy_max INTEGER, following_min INTEGER, following_max INTEGER,
    between_min INTEGER, between_max INTEGER, tail_min INTEGER, tail_max INTEGER,
    tail_settled_min INTEGER, tail_settled_max INTEGER,
    expected_hard REAL, probability_hard REAL, probability_hard_choke REAL,
    expected_tail_given_hard REAL, expected_new_easy REAL,
    PRIMARY KEY(puzzle_id,phase,threshold),
    FOREIGN KEY(puzzle_id,phase) REFERENCES story_runs(puzzle_id,phase) ON DELETE CASCADE);
CREATE TABLE IF NOT EXISTS story_availability(
    puzzle_id TEXT, phase TEXT, state INTEGER,
    remaining_literals INTEGER, cheapest_literals INTEGER, literal_fraction REAL,
    unresolved_variables INTEGER, cheapest_variables INTEGER, variable_fraction REAL,
    visit_probability REAL,
    PRIMARY KEY(puzzle_id,phase,state),
    FOREIGN KEY(puzzle_id,phase) REFERENCES story_runs(puzzle_id,phase) ON DELETE CASCADE);
CREATE TABLE IF NOT EXISTS story_independence(
    puzzle_id TEXT, phase TEXT, state INTEGER, width INTEGER,
    diamonds INTEGER, stable_pairs INTEGER, local_groups INTEGER, first_common_state INTEGER,
    PRIMARY KEY(puzzle_id,phase,state),
    FOREIGN KEY(puzzle_id,phase) REFERENCES story_runs(puzzle_id,phase) ON DELETE CASCADE);
CREATE TABLE IF NOT EXISTS story_profiles(
    puzzle_id TEXT, phase TEXT, profile INTEGER, mus_size INTEGER, family_multiset TEXT,
    expected_uses REAL, probability_seen REAL, unavoidable INTEGER,
    PRIMARY KEY(puzzle_id,phase,profile),
    FOREIGN KEY(puzzle_id,phase) REFERENCES story_runs(puzzle_id,phase) ON DELETE CASCADE);
CREATE VIEW IF NOT EXISTS current_story_statistics AS
    SELECT p.game,p.source_tier,p.source_pack,p.board_area,s.* FROM story_runs s
    JOIN puzzles p ON p.id=s.puzzle_id
    JOIN runs r ON r.puzzle_id=s.puzzle_id AND r.phase=s.phase
    WHERE s.archive_sha256=r.archive_sha256 AND r.status='complete'
      AND s.status IN ('complete','partial');
'''


def index(db, source, record, result=None):
    fields = ['version', 'archive_sha256', 'status', 'computed_utc', 'seconds',
              'statistics_path', 'statistics_sha256', 'report_path', 'error',
              'route_count', 'moves_min', 'moves_max', 'peak_min', 'peak_max', 'chain_min', 'chain_max']
    if result:
        record.update(route_count=result['state_routes'],
                      moves_min=result['moves']['min'], moves_max=result['moves']['max'],
                      peak_min=result['peak_mus']['min'], peak_max=result['peak_mus']['max'],
                      chain_min=result['release']['chains'].get('min'),
                      chain_max=result['release']['chains'].get('max'))
    key = (source['puzzle_id'], source['phase'])
    with db:
        db.execute('INSERT OR REPLACE INTO story_runs(puzzle_id,phase,' + ','.join(fields) + ') VALUES (' +
                   ','.join('?' for _ in range(len(fields) + 2)) + ')', key + tuple(record.get(f) for f in fields))
        if result:
            for row, choke in zip(result['hard_moves'], result['chokes']['by_threshold']):
                db.execute('INSERT INTO story_thresholds VALUES (?,?,?,?,?,?,?,?,?,?,?,?)', key + (
                    row['size_at_least'], row['min'], row['max'], row['gap'],
                    row['longest_streak']['min'], row['longest_streak']['max'],
                    choke['min'], choke['max'], row['min_route'], row['max_route']))
            for row in result['chokes']['states']:
                db.execute('INSERT INTO story_chokes VALUES (?,?,?,?,?,?,?,?)', key + (
                    row['state'], row['mus_size'], int(row['unavoidable_state']),
                    row['deduction_width'], row['batch_width'], row['explanation_width']))
            for row in result['chokes']['width_size_histogram']:
                db.execute('INSERT INTO story_widths VALUES (?,?,?,?,?)', key + (
                    row['width'], row['mus_size'], row['states']))
            for row in result['chokes']['narrow_routes']:
                db.execute('INSERT INTO story_narrow VALUES (?,?,?,?,?,?,?)', key + (
                    row['width_at_most'], row['encounters']['min'], row['encounters']['max'],
                    row['longest_streak']['min'], row['longest_streak']['max']))
            index_flow(db, key, result['flow'])


def index_flow(db, key, flow):
    ind, profiles = flow['independence'], flow['proof_profiles']
    db.execute('INSERT INTO story_flow VALUES (' + ','.join('?' for _ in range(15)) + ')', key + (
        flow['version'], flow['random_policy'], flow['metadata']['variable_mapping'], profiles['status'],
        flow['expected_moves'], profiles['expected_distinct_profiles'], profiles.get('expected_repeated_profile_uses'),
        ind['union_diamonds'], ind['stable_disjoint_pairs'], ind['peak_local_groups']['min'],
        ind['peak_local_groups']['max'], ind['multi_group_moves']['min'], ind['multi_group_moves']['max']))
    for r in flow['thresholds']:
        tail = r['routine_tail_moves']['with_hard_move']
        cells = r.get('routine_tail_settled_variables', {}).get('with_hard_move', {})
        db.execute('INSERT INTO story_pacing VALUES (' + ','.join('?' for _ in range(18)) + ')', key + (
            r['hard_threshold'], r['newly_easy_literals']['min'], r['newly_easy_literals']['max'],
            r['following_easy_moves']['min'], r['following_easy_moves']['max'],
            r['between_hard_easy_moves']['min'], r['between_hard_easy_moves']['max'], tail['min'], tail['max'],
            cells.get('min'), cells.get('max'), r['expected_hard_moves'], r['probability_any_hard_move'],
            r['probability_any_hard_choke'], tail['expected_conditional'], r['expected_newly_easy_literals']))
    db.executemany('INSERT INTO story_availability VALUES (?,?,?,?,?,?,?,?,?,?)', [key + (
        r['state'], r['remaining_literals'], r['cheapest_literals'], r['cheapest_literal_fraction'],
        r['unresolved_variables'], r['cheapest_variables'], r['cheapest_variable_fraction'], r['visit_probability'])
        for r in flow['availability']])
    db.executemany('INSERT INTO story_independence VALUES (?,?,?,?,?,?,?,?)', [key + (
        r['state'], r['width'], r['union_diamonds'], r['stable_disjoint_pairs'], r['local_group_count'], r['first_common_state'])
        for r in ind['states']])
    db.executemany('INSERT INTO story_profiles VALUES (?,?,?,?,?,?,?,?)', [key + (
        r['profile'], r['mus_size'], json.dumps(r['family_multiset']), r.get('expected_uses'), r.get('probability_seen'),
        int(r['unavoidable_profile']) if 'unavoidable_profile' in r else None) for r in profiles['profiles']])


def catalog(root, db):
    rows = db.execute('''SELECT p.game,s.* FROM story_runs s JOIN puzzles p ON p.id=s.puzzle_id
                         ORDER BY p.game,s.puzzle_id,s.phase''').fetchall()
    counts = {}
    for row in rows:
        counts[row['status']] = counts.get(row['status'], 0) + 1
    lines = ['# Puzzle story statistics', '',
             ', '.join(f'{n} {status}' for status, n in sorted(counts.items())) + '.', '',
             'All measurements cover ties between current cheapest moves. No more expensive '
             'choices were added and no solver searches were run. Unfinished or ambiguous graphs '
             'are excluded from comparisons. A partial result has exact path/choke counts but '
             'an explicitly unavailable release-chain bound.', '',
             '[SQLite database](../corpus.sqlite) · [Generation catalog](../README.md)', '',
             'The database adds `story_runs`, `story_thresholds`, `story_chokes`, `story_widths`, '
             '`story_narrow`, `story_flow`, `story_pacing`, `story_availability`, '
             '`story_independence`, and `story_profiles`. `current_story_statistics` joins source difficulty and excludes '
             'results whose recorded graph hash no longer matches the latest run. '
             'Difficulty tiers use different game encodings; this is an exploratory selected sample.', '',
             '“Chain” is the min–max longest release depth over complete routes. '
             'A release records the last transition that made a deduction’s eventual MUS size '
             'available. It can follow joint prerequisites and alternate enabling routes; '
             'it does not certify that the last move was an individually necessary prerequisite.', '',
             '| Puzzle | Pass | Status | Peak MUS | Moves | Release chain | Largest hard-count gap | Report |',
             '| --- | --- | --- | ---: | ---: | ---: | ---: | --- |']
    for row in rows:
        def interval(prefix):
            lo, hi = row[prefix + '_min'], row[prefix + '_max']
            return '—' if lo is None else str(lo) if lo == hi else f'{lo}–{hi}'
        gap = db.execute('SELECT max(hard_gap) FROM story_thresholds WHERE puzzle_id=? AND phase=?',
                         (row['puzzle_id'], row['phase'])).fetchone()[0]
        link = ('[details](' + os.path.relpath(root / row['report_path'], root / 'statistics') + ')'
                if row['report_path'] else row['error'] or '')
        lines.append(f"| {row['puzzle_id']} | {row['phase']} | {row['status']} | {interval('peak')} | "
                     f"{interval('moves')} | {interval('chain')} | {gap if gap is not None else '—'} | {link} |")
    save(root / 'statistics/README.md', '\n'.join(lines) + '\n')


def provenance(path, graph_hash):
    return {'archive': str(path), 'archive_sha256': graph_hash,
            'analyser_sha256': digest(HERE / 'story.py'), 'driver_sha256': digest(Path(__file__)),
            'flow_analyser_sha256': digest(HERE / 'flow.py'),
            'computed_utc': datetime.datetime.now(datetime.timezone.utc).isoformat()}


def corpus(args):
    root = args.corpus.resolve()
    db = sqlite3.connect(root / 'corpus.sqlite')
    db.row_factory = sqlite3.Row
    db.execute('PRAGMA foreign_keys=ON')
    db.executescript(SCHEMA)
    sources = db.execute('SELECT * FROM runs ORDER BY puzzle_id,phase').fetchall()
    selected = [r for r in sources if (args.phase is None or r['phase'] == args.phase)
                and (args.ids is None or r['puzzle_id'] in args.ids)]
    unknown = set(args.ids or []) - {r['puzzle_id'] for r in selected}
    if unknown:
        raise ValueError(f'Unknown puzzle IDs in selected phase: {sorted(unknown)}')
    if not selected:
        raise ValueError('No matching corpus runs')
    for source in selected:
        started = time.monotonic()
        identifier, phase = source['puzzle_id'], source['phase']
        record = {'version': VERSION, 'archive_sha256': source['archive_sha256'],
                  'computed_utc': datetime.datetime.now(datetime.timezone.utc).isoformat()}
        result = None
        try:
            if source['status'] != 'complete':
                raise IncompleteGraph('Source run is ' + source['status'])
            path = root / source['archive_path']
            sha = digest(path)
            if sha != source['archive_sha256']:
                raise ValueError('Archive hash differs from the generation database; reindex it first')
            graph = read_json(path)
            result = analyse(graph, max_labels=args.max_labels)
            result['source'] = provenance(path, sha)
            result['source'].update(puzzle_id=identifier, phase=phase)
            out = root / 'statistics' / phase / f'{identifier}.story.json.zst'
            md = report_path(out)
            # Machine-readable rows can be large; Markdown is the readable report.
            save(out, json.dumps(result, separators=(',', ':')) + '\n')
            viewer = os.path.relpath(root / source['viewer_path'], md.parent) if source['viewer_path'] else None
            save(md, report(result, graph, identifier + ' · ' + phase, viewer))
            record.update(status=result['status'], statistics_path=str(out.relative_to(root)),
                          statistics_sha256=digest(out), report_path=str(md.relative_to(root)))
        except IncompleteGraph as error:
            record.update(status='excluded', error=str(error))
        except Exception as error:
            result = None
            record.update(status='error', error=str(error))
        record['seconds'] = round(time.monotonic() - started, 4)
        index(db, source, record, result)
        print(identifier, phase, record['status'], f"{record['seconds']:.3f}s", record.get('error', ''), flush=True)
    catalog(root, db)
    assert db.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
    assert not db.execute('PRAGMA foreign_key_check').fetchall()
    failed = db.execute("SELECT count(*) FROM story_runs WHERE status='error'").fetchone()[0]
    db.close()
    if failed:
        raise SystemExit(f'{failed} analysis errors; see story_runs.error')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    inputs = parser.add_mutually_exclusive_group(required=True)
    inputs.add_argument('--input', type=Path)
    inputs.add_argument('--corpus', type=Path)
    parser.add_argument('--out', type=Path, help='Single-graph JSON output (also writes Markdown); otherwise stdout')
    parser.add_argument('--phase', help='Optional corpus phase filter')
    parser.add_argument('--ids', nargs='+', help='Optional corpus puzzle IDs')
    parser.add_argument('--max-labels', type=int, default=100000,
                        help='Release-chain history limit per bound; 0 disables. Other metrics remain exact.')
    args = parser.parse_args()
    if args.max_labels < 0:
        parser.error('--max-labels must be nonnegative')
    if args.corpus and args.out:
        parser.error('--out is only for single-graph analysis')
    if args.input and (args.phase or args.ids):
        parser.error('--phase and --ids require --corpus')
    if args.corpus:
        corpus(args)
        return
    if args.out:
        targets = [args.out.resolve(), report_path(args.out).resolve()]
        if args.input.resolve() in targets or targets[0] == targets[1]:
            parser.error('JSON/report outputs must be distinct and must not replace the source graph')
    raw = read_bytes(args.input)
    graph = json.loads(raw)
    result = analyse(graph, max_labels=args.max_labels)
    result['source'] = provenance(args.input, digest(args.input))
    encoded = json.dumps(result, separators=(',', ':')) + '\n'
    if args.out:
        save(args.out, encoded)
        save(report_path(args.out), report(result, graph, args.input.stem))
    else:
        print(encoded, end='')


if __name__ == '__main__':
    main()
