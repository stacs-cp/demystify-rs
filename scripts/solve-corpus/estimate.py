#!/usr/bin/env python3
"""Estimate capped first-pass work, retaining slow/unfinished puzzle observations.

These are engineering projections from selected samples, not confidence bounds
or predictions of time to complete every uncapped deduction graph.
"""

import argparse
from collections import Counter, defaultdict
import datetime
import json
from pathlib import Path
import sqlite3
import statistics

from expand import fingerprint, size_band


def observations(roots, current_hashes, budget):
    result = {}
    for root in roots:
        manifest = json.loads((root / 'manifest.json').read_text())
        hashes = {i['id']: i.get('content_sha256') or fingerprint(json.loads((root / i['input']).read_text()))
                  for i in manifest['instances']}
        with sqlite3.connect(root / 'corpus.sqlite') as db:
            db.row_factory = sqlite3.Row
            rows = db.execute("SELECT * FROM attempts WHERE phase='baseline' AND status<>'running' ORDER BY started_utc").fetchall()
        seen = set()
        for row in rows:
            identifier = row['puzzle_id']
            settings = json.loads(row['settings_json'] or '{}')
            if identifier in seen or not settings.get('command'):
                continue
            seen.add(identifier)
            content = hashes[identifier]
            if content not in current_hashes or settings.get('budget_seconds', 0) < budget:
                continue
            # Only initial builds describe a fresh pass. A short successful
            # resume must not masquerade as the entire graph's generation time.
            if 'build' not in settings['command']:
                continue
            seconds = row['seconds']
            if seconds is None:
                continue
            terminal = row['status'] in ('complete', 'underdetermined') and seconds <= budget
            value = {'puzzle_id': identifier, 'corpus': str(root), 'status': row['status'],
                     'seconds': seconds, 'capped_seconds': seconds if terminal else budget,
                     'complete_within_budget': row['status'] == 'complete' and seconds <= budget,
                     'unresolved_at_budget': not terminal,
                     'invalid_nonunique': row['status'] == 'underdetermined'}
            result.setdefault(content, value)
    return result


def project(inventory, samples, budget=60, workers=2):
    population, groups = Counter(), defaultdict(list)
    for item in inventory['entries']:
        key = (item['game'], size_band(item['board_area']))
        population[key] += 1
        if item['content_sha256'] in samples:
            groups[key].append(samples[item['content_sha256']])
    rows = []
    projected, unfinished, unknown = 0.0, 0.0, 0
    for (game, band), count in sorted(population.items()):
        values = groups[(game, band)]
        mean = statistics.mean(v['capped_seconds'] for v in values) if values else None
        rate = statistics.mean(v['unresolved_at_budget'] for v in values) if values else None
        if values:
            projected += count * mean
            unfinished += count * rate
        else:
            unknown += count
        rows.append({'game': game, 'size_band': band, 'population': count, 'samples': len(values),
                     'complete_within_budget': sum(v['complete_within_budget'] for v in values),
                     'nonunique': sum(v.get('invalid_nonunique', False) for v in values),
                     'unresolved_at_budget': sum(v['unresolved_at_budget'] for v in values),
                     'mean_capped_seconds': mean, 'unresolved_rate': rate})
    total = sum(population.values())
    return {
        'scope': 'Approximate elapsed job time for a fresh capped pass; serial export is additional. '
                 'Successful job timings include inspection/rendering overhead.',
        'warning': 'Selected samples, few observations per stratum; not a statistical confidence interval. '
                   'Capped completion is not uncapped fixed-point completion.',
        'budget_seconds': budget, 'workers': workers, 'source_entries': total,
        'observed_current_instances': len(samples), 'unmeasured_population': unknown,
        'projected_solver_hours_measured_strata': projected / workers / 3600,
        'projected_solver_hours_if_unmeasured_use_full_budget': (projected + unknown * budget) / workers / 3600,
        'projected_unfinished_measured_strata': unfinished,
        'all_jobs_use_full_budget_solver_hours': total * budget / workers / 3600,
        'rows': rows,
    }


def markdown(result):
    b, w = result['budget_seconds'], result['workers']
    lines = ['# Current-puzzle generation estimate', '',
             f"{result['source_entries']} source entries; {result['observed_current_instances']} measured current instances. "
             f'Projection uses a {b}-second first-pass budget and {w} concurrent puzzle jobs.', '',
             result['warning'], '',
             f"Reweighting the observed capped times by game and board-size band gives "
             f"**{result['projected_solver_hours_measured_strata']:.2f} hours elapsed** for measured strata. "
             f"There are {result['unmeasured_population']} entries in unmeasured strata; assigning those the full "
             f"budget raises the projection to **{result['projected_solver_hours_if_unmeasured_use_full_budget']:.2f} hours**.", '',
             f"The corresponding projection leaves about {result['projected_unfinished_measured_strata']:.0f} "
             'entries unfinished in the measured strata. This is a rough extrapolation, not a promised completion count.', '',
             f"If every entry uses the full solver budget, the allocation is "
             f"**{result['all_jobs_use_full_budget_solver_hours']:.2f} hours** with {w} workers. "
             'Export/uniqueness checks run serially and add time; archive inspection and rendering also add overhead. '
             'Fast completions can save most of that allocation. The database timings themselves include '
             'some inspection/rendering, so the reweighted number is only an engineering approximation.', '',
             'Completed-only averages exclude the difficult tail and are not used for the projection. '
             'Only initial build attempts with a sufficient observation budget are included; successful resumes '
             'are never counted as fresh fast solves. Inputs must match the current inventory by full JSON content. '
             'Nonunique terminal cases are reported separately from completed graphs. Failed or unfinished '
             'native jobs use the full budget in the projection.', '',
             '| Game | Board-size band | Current entries | Samples | Complete within budget | Nonunique | Unresolved at budget | Mean capped seconds |',
             '| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |']
    for r in result['rows']:
        mean = f"{r['mean_capped_seconds']:.2f}" if r['mean_capped_seconds'] is not None else 'unmeasured'
        lines.append(f"| {r['game']} | {r['size_band']} | {r['population']} | {r['samples']} | "
                     f"{r['complete_within_budget']} | {r['nonunique']} | {r['unresolved_at_budget']} | {mean} |")
    lines += ['', 'Board area is a rough size measure. For example, Seasons has four layers of variables '
              'on each board. Graph growth also depends on independent deductions, not just area or MUS size. '
              'No defensible finite completion-time estimate is available for all uncapped graphs from these samples.', '']
    return '\n'.join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inventory', type=Path, required=True)
    parser.add_argument('--corpus', type=Path, action='append', required=True)
    parser.add_argument('--budget', type=int, default=60)
    parser.add_argument('--workers', type=int, default=2)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if args.budget < 1 or args.workers < 1:
        parser.error('budget and workers must be positive')
    inventory = json.loads(args.inventory.read_text())
    samples = observations(args.corpus, {i['content_sha256'] for i in inventory['entries']}, args.budget)
    result = project(inventory, samples, args.budget, args.workers)
    result['computed_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    result['observations'] = samples
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + '\n')
    args.out.with_suffix('.md').write_text(markdown(result))
    print(f"{len(samples)} observations; projected capped time "
          f"{result['projected_solver_hours_measured_strata']:.2f}–"
          f"{result['projected_solver_hours_if_unmeasured_use_full_budget']:.2f}h; "
          f"{result['unmeasured_population']} entries in unmeasured strata")


if __name__ == '__main__':
    main()
