#!/usr/bin/env python3
"""Inventory current level files and prepare a broader, reproducible graph sample."""

import argparse
from collections import Counter, defaultdict, deque
import datetime
import hashlib
import json
from pathlib import Path
import random
import re


BANDS = ('up-to-20', '21-to-36', 'over-36', 'unknown')


def fingerprint(data):
    return hashlib.sha256(json.dumps(data, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def area(data, game):
    if game == 'combination':
        grid = data['grid'].split('|') if isinstance(data['grid'], str) else data['grid']
        return sum(map(len, grid))
    board = data.get('board', {})
    board = board if isinstance(board, dict) else {}
    value = (data.get('width', 0) * data.get('height', 0) or data.get('size', 0) ** 2
             or board.get('L', 0) * board.get('M', 0))
    if value:
        return value
    if 'radius' in data:
        return 1 + 3 * data['radius'] * (data['radius'] + 1)
    return None


def size_band(value):
    return 'unknown' if value is None else 'up-to-20' if value <= 20 else '21-to-36' if value <= 36 else 'over-36'


def tier(data, pack):
    meta = data.get('meta', {}) or {}
    difficulty = meta.get('difficulty', {})
    difficulty = difficulty if isinstance(difficulty, dict) else {}
    value = meta.get('mus', meta.get('target_mus', difficulty.get('mus_quality')))
    if value is not None:
        try:
            return int(value)
        except (TypeError, ValueError):
            pass
    found = re.search(r'mus0*(\d+)', pack)
    return int(found[1]) if found else None


def variant(game, pack):
    if game in ('bloomsweeper', 'fell'):
        return pack
    if game == 'gamut':
        return 'tinted' if pack.startswith('tinted') else 'plain'
    if game == 'sashes':
        return 'hex' if pack.startswith('hex') else 'rhombus'
    return 'main'


def inventory(apps):
    items = []
    for app in sorted((apps / 'apps').iterdir()):
        if not app.is_dir():
            continue
        files = [(p, None, json.loads(p.read_text()))
                 for p in sorted(app.glob('src/game/levelpacks/*/*.json'))]
        if app.name == 'combination':
            files += [(p, i, data) for p in sorted(app.glob('public/assets/levels/*.json'))
                      for i, data in enumerate(json.loads(p.read_text()))]
        for path, index, data in files:
            pack = path.stem if index is not None else path.parent.name
            name = str(index + 1) if index is not None else path.stem
            identifier = re.sub(r'[^a-zA-Z0-9_.-]+', '-', f'{app.name}-{pack}-{name}')
            a = area(data, app.name)
            items.append({'id': identifier, 'game': app.name, 'variant': variant(app.name, pack),
                          'source_pack': pack, 'source_tier': tier(data, pack), 'board_area': a,
                          'size_band': size_band(a), 'original_path': str(path),
                          **({'original_array_index': index} if index is not None else {}),
                          'content_sha256': fingerprint(data), 'data': data})
    if len({i['id'] for i in items}) != len(items):
        raise ValueError('Source identifiers collide')
    return items


def selection(items, excluded, per_game, seed, max_area=None):
    by_game = defaultdict(list)
    for item in items:
        if item['content_sha256'] in excluded:
            continue
        if max_area is not None and (item['board_area'] is None or item['board_area'] > max_area):
            continue
        by_game[item['game']].append(item)
    chosen = []
    for game, candidates in sorted(by_game.items()):
        strata = defaultdict(lambda: defaultdict(list))
        for item in candidates:
            strata[(item['variant'], item['size_band'])][item['source_pack']].append(item)
        queues = []
        for key, packs in sorted(strata.items()):
            rng = random.Random(f'{seed}:{game}:{key}')
            pack_names = sorted(packs)
            rng.shuffle(pack_names)
            for name in sorted(packs):
                values = packs[name]
                values.sort(key=lambda i: i['id'])
                rng.shuffle(values)
            queue = deque()
            while any(packs.values()):
                for name in pack_names:
                    if packs[name]:
                        queue.append(packs[name].pop())
            queues.append(queue)
        # At least one per variant/size stratum; then round-robin through strata
        # and shuffled packs. This is deliberately stratified, not a uniform
        # random sample of all levels or an assertion of calibrated difficulty.
        quota = min(len(candidates), max(per_game, len(queues)))
        added = 0
        while added < quota:
            for queue in queues:
                if queue and added < quota:
                    chosen.append(queue.popleft())
                    added += 1
    return chosen


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apps-root', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--exclude-corpus', type=Path, action='append', default=[])
    parser.add_argument('--per-game', type=int, default=6)
    parser.add_argument('--seed', type=int, default=20260913)
    parser.add_argument('--max-area', type=int)
    parser.add_argument('--all', action='store_true', help='Select every remaining entry instead of a sample')
    args = parser.parse_args()
    if args.per_game < 1 or (args.max_area is not None and args.max_area < 1):
        parser.error('per-game and max-area must be positive')
    root, apps = args.out.resolve(), args.apps_root.resolve()
    if (root / 'manifest.json').exists():
        parser.error('Manifest exists; use a new corpus directory')
    items = inventory(apps)
    excluded = set()
    for previous in args.exclude_corpus:
        manifest = json.loads((previous / 'manifest.json').read_text())
        excluded.update(fingerprint(json.loads((previous / item['input']).read_text()))
                        for item in manifest['instances'])
    eligible = [i for i in items if i['content_sha256'] not in excluded
                and (args.max_area is None or (i['board_area'] is not None and i['board_area'] <= args.max_area))]
    chosen = eligible if args.all else selection(items, excluded, args.per_game, args.seed, args.max_area)
    if not chosen:
        parser.error('No new matching levels')
    for folder in ('inputs', 'parsed', 'graphs', 'viewers', 'logs', 'validation', 'sources'):
        (root / folder).mkdir(parents=True, exist_ok=True)
    counts = {}
    for game in sorted({i['game'] for i in items}):
        entries = [i for i in items if i['game'] == game]
        counts[game] = {'available_levels': len(entries),
                        'packs': dict(Counter(i['source_pack'] for i in entries)),
                        'size_bands': dict(Counter(i['size_band'] for i in entries))}
    records, source_hashes = [], {}
    for item in chosen:
        record = {k: v for k, v in item.items() if k != 'data'}
        path = root / 'inputs' / (item['id'] + '.json')
        path.write_text(json.dumps(item['data'], indent=2) + '\n')
        source = item['original_path']
        if source not in source_hashes:
            source_hashes[source] = hashlib.sha256(Path(source).read_bytes()).hexdigest()
        record.update(input=str(path.relative_to(root)), input_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                      original_sha256=source_hashes[source])
        records.append(record)
    description = ('All remaining source entries' if args.all else
                   'Round-robin variant/board-size strata, then seeded shuffled packs and levels; '
                   'at least one new entry per nonempty stratum. Larger boards are included unless max-area is set.')
    manifest = {'schema_version': 1, 'created_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                'apps_root': str(apps), 'selection': description,
                'selection_settings': {'seed': args.seed, 'per_game': args.per_game, 'max_area': args.max_area,
                                       'excluded_corpora': [str(p.resolve()) for p in args.exclude_corpus]},
                'settings': {'repeats': 5, 'strategy': 'dynamic', 'conflict_limit': 1000,
                             'only_assign': True, 'threads': 2, 'checkpoint_seconds': 10},
                'inventory': counts, 'instances': records}
    (root / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    (root / 'inventory.json').write_text(json.dumps({
        'created_utc': manifest['created_utc'], 'apps_root': str(apps),
        'source_entries': len(items), 'unique_full_json_contents': len({i['content_sha256'] for i in items}),
        'games': counts, 'entries': [{k: v for k, v in i.items() if k != 'data'} for i in items],
    }, indent=2) + '\n')
    print(f'{len(items)} source entries in {len(counts)} games; selected {len(records)} new instances')
    for game in sorted({r['game'] for r in records}):
        subset = [r for r in records if r['game'] == game]
        print(game, len(subset), dict(Counter(r['size_band'] for r in subset)))


if __name__ == '__main__':
    main()
