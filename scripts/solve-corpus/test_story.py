"""Analytic fixtures and exhaustive small-graph oracles for story statistics."""

import copy
import itertools
import random
import unittest
import contextlib
import hashlib
import io
import json
from pathlib import Path
from types import SimpleNamespace
import sqlite3
import tempfile

import analyse as driver

from story import Story, analyse, IncompleteGraph, InvalidGraph


def fixture(n, cost, duplicate=False):
    universe = set(range(1, n + 1))
    proof_costs = set()
    for bits in itertools.product([False, True], repeat=n):
        known = frozenset(i + 1 for i, bit in enumerate(bits) if bit)
        for lit in universe - known:
            c = cost(known, lit)
            if c is not None:
                proof_costs.add((lit, c))
    specs = sorted(proof_costs)
    if duplicate:
        specs += specs[:1]
    proofs = [{'constraints': list(range(1000 + 100 * p, 1000 + 100 * p + c)),
               'discovered_at': 0, 'reduced_from': None} for p, (_, c) in enumerate(specs)]
    states, nodes = [frozenset()], []
    ids = {states[0]: 0}
    for known in states:
        remaining = sorted(universe - known)
        evidence = {}
        for p, (lit, size) in enumerate(specs):
            if lit in remaining and cost(known, lit) is not None and cost(known, lit) <= size:
                evidence[str(p)] = [lit]
        minimum = min((len(proofs[int(p)]['constraints']) for p in evidence), default=None)
        edges = []
        for p, lits in evidence.items():
            if len(proofs[int(p)]['constraints']) != minimum:
                continue
            child = known | set(lits)
            if child not in ids:
                ids[child] = len(states)
                states.append(child)
            edges.append({'target': ids[child], 'proof': int(p), 'deduced': lits,
                          'implied': [], 'active': True})
        nodes.append({'known': sorted(known), 'remaining': remaining,
                      'unresolved_candidates': len(remaining),
                      'status': 'open' if edges else 'solved' if not remaining else 'underdetermined',
                      'evidence': evidence, 'edges': edges, 'checked_proofs': len(proofs),
                      'needs_expansion': False})
    return {'format': 'demystify-solve-graph', 'version': 1, 'fixed_point': True,
            'root': 0, 'nodes': nodes, 'proofs': proofs, 'puzzle': {}}


def routes(story, u=None):
    if u is None:
        u = story.root
    if not story.moves[u]:
        yield ()
    for move in story.moves[u]:
        for suffix in routes(story, move.target):
            yield (move.key,) + suffix


def reference_depth(story, route):
    """Walk backwards independently for each played deduction's final cost."""
    states, depths = [story.root], []
    for key in route:
        move = story.by_key[key]
        causes = []
        for lit in move.deduced:
            first = next(i for i, u in enumerate(states)
                         if story.best[u].get(lit, float('inf')) <= move.size)
            causes.append(1 if first == 0 else depths[first - 1] + 1)
        depths.append(max(causes))
        states.append(move.target)
    return max(depths, default=0)


def longest_run(values):
    best = current = 0
    for value in values:
        current = current + 1 if value else 0
        best = max(best, current)
    return best


class StoryTests(unittest.TestCase):
    def test_independent_choices_and_unavoidable_chokes_at_different_states(self):
        result = analyse(fixture(3, lambda s, lit: 3))
        self.assertEqual(result['state_routes'], '6')
        self.assertEqual((result['release']['chains']['min'], result['release']['chains']['max']), (1, 1))
        self.assertEqual(result['hard_moves'][2]['min'], 3)
        self.assertEqual(result['chokes']['narrow_routes'][0]['encounters']['min'], 1)
        self.assertEqual(result['chokes']['narrow_routes'][0]['encounters']['max'], 1)
        self.assertTrue(all(not s['unavoidable_state'] for s in result['chokes']['states']))

    def test_linear_release_chain(self):
        cost = lambda s, lit: {1: 3, 2: 2 if 1 in s else 6, 3: 1 if 2 in s else 7}[lit]
        result = analyse(fixture(3, cost))
        self.assertEqual(result['release']['chains']['min'], 3)
        self.assertEqual(result['release']['chains']['max'], 3)
        self.assertEqual(result['state_routes'], '1')
        self.assertTrue(all(s['unavoidable_state'] for s in result['chokes']['states']))
        self.assertEqual(result['hard_moves'][1]['longest_streak']['min'], 2)

    def test_tie_breaking_changes_hard_work_with_same_peak(self):
        def cost(s, lit):
            return {1: 5, 2: 1 if {1, 3} <= s else 5, 3: 1 if 1 in s else 6}[lit]
        result = analyse(fixture(3, cost))
        self.assertEqual((result['peak_mus']['min'], result['peak_mus']['max']), (5, 5))
        self.assertEqual((result['hard_moves'][4]['min'], result['hard_moves'][4]['max']), (1, 2))
        self.assertEqual((result['release']['chains']['min'], result['release']['chains']['max']), (2, 3))
        for key, expected in [('min_route', [5, 1, 1]), ('max_route', [5, 5, 1])]:
            self.assertEqual(result['witnesses'][result['hard_moves'][4][key]]['difficulty_sequence'], expected)

    def test_queue_promotion_is_not_release(self):
        result = analyse(fixture(2, lambda s, lit: lit))
        self.assertEqual(result['release']['chains']['max'], 1)
        self.assertEqual(result['release']['cost_drop_moves']['max'], 0)
        self.assertEqual(result['release']['queue_promotion_moves']['min'], 1)

    def test_chain_survives_intervening_independent_moves(self):
        def cost(s, lit):
            return {1: 1, 2: 1, 3: 1 if 1 in s else 4, 4: 1 if 3 in s else 4}[lit]
        story = Story(fixture(4, cost))
        chosen, u = [], story.root
        for lit in [1, 2, 3, 4]:
            move = next(m for m in story.moves[u] if m.deduced == (lit,))
            chosen.append(move.key)
            u = move.target
        name = story.intern(chosen)
        story.annotate_route(story.witnesses[name])
        self.assertEqual(story.witnesses[name]['release']['longest_chain_steps'], [0, 2, 3])
        self.assertEqual(story.witnesses[name]['release']['events'][2]['parents'], [0])

    def test_or_and_joint_enabling_are_path_specific(self):
        for enabled in [lambda s: bool({1, 2} & s), lambda s: {1, 2} <= s]:
            graph = fixture(3, lambda s, lit: (1 if enabled(s) else 4) if lit == 3 else 2)
            result = analyse(graph)
            self.assertEqual((result['release']['chains']['min'], result['release']['chains']['max']), (2, 2))

    def test_multiple_explanations_do_not_widen_or_multiply_routes(self):
        result = analyse(fixture(1, lambda s, lit: 2, duplicate=True))
        self.assertEqual(result['state_routes'], '1')
        self.assertEqual(result['chokes']['states'][0]['explanation_width'], 2)
        self.assertEqual(result['chokes']['states'][0]['batch_width'], 1)
        self.assertEqual(result['moves']['min'], 1)

    def test_batched_deductions_count_once(self):
        graph = fixture(3, lambda s, lit: 1 if lit == 3 and {1, 2} <= s else 2 if lit in [1, 2] else 4)
        root = graph['nodes'][0]
        combined = next(i for i, n in enumerate(graph['nodes']) if n['known'] == [1, 2])
        for p in list(root['evidence']):
            if len(graph['proofs'][int(p)]['constraints']) == 2:
                root['evidence'][p] = [1, 2]
        for edge in root['edges']:
            edge['deduced'] = [1, 2]
            edge['target'] = combined
        result = analyse(graph)
        self.assertEqual(result['moves']['max'], 2)
        self.assertEqual(result['hard_moves'][1]['max'], 1)
        self.assertEqual(result['release']['chains']['max'], 2)
        self.assertEqual(result['chokes']['states'][0]['deduction_width'], 2)

    def test_excludes_historical_nodes_and_superseded_edges(self):
        graph = fixture(2, lambda s, lit: 1)
        historical = copy.deepcopy(graph['nodes'][0])
        historical['known'] = [-100]
        historical['status'] = 'pending'
        historical['needs_expansion'] = True
        graph['nodes'].append(historical)
        edge = dict(graph['nodes'][0]['edges'][0], target=len(graph['nodes']) - 1, active=False)
        graph['nodes'][0]['edges'].append(edge)
        result = analyse(graph)
        self.assertEqual(result['active_states'], 4)
        self.assertEqual(result['retained_states'], 5)

    def test_rejects_unfinished_and_invalid_inputs(self):
        graph = fixture(1, lambda s, lit: 1)
        checkpoint = dict(graph, fixed_point=False)
        with self.assertRaises(IncompleteGraph):
            analyse(checkpoint)
        ambiguous = fixture(1, lambda s, lit: None)
        with self.assertRaises(IncompleteGraph):
            analyse(ambiguous)
        graph['nodes'][0]['edges'][0]['target'] = 0
        with self.assertRaises(InvalidGraph):
            analyse(graph)

    def test_solved_root_and_explicit_chain_resource_limit(self):
        result = analyse(fixture(0, lambda s, lit: 1))
        self.assertEqual(result['moves']['max'], 0)
        self.assertEqual(result['release']['chains']['min'], 0)
        self.assertEqual(result['hard_moves'], [])
        result = analyse(fixture(3, lambda s, lit: 1), max_labels=1)
        self.assertEqual(result['status'], 'partial')
        self.assertEqual(result['release']['chains']['status'], 'resource_limit')
        self.assertNotIn('min', result['release']['chains'])
        self.assertEqual(result['hard_moves'][0]['min'], 3)

    def test_node_ids_need_not_be_topological(self):
        graph = fixture(4, lambda s, lit: max(1, lit - len(s)))
        expected = analyse(graph)
        count = len(graph['nodes'])
        graph['nodes'].reverse()
        graph['root'] = count - 1
        for node in graph['nodes']:
            for edge in node['edges']:
                edge['target'] = count - 1 - edge['target']
        actual = analyse(graph)
        self.assertEqual(expected['state_routes'], actual['state_routes'])
        self.assertEqual(expected['release']['chains']['min'], actual['release']['chains']['min'])
        self.assertEqual(expected['release']['chains']['max'], actual['release']['chains']['max'])

    def test_route_counts_exceed_64_bits_without_enumeration(self):
        layers = 70
        universe = set(range(1, 2 * layers + 1))
        proofs = [{'constraints': [1000 + i], 'discovered_at': 0, 'reduced_from': None}
                  for i in range(2 * layers)]
        def node(known):
            remaining = sorted(universe - known)
            return {'known': sorted(known), 'remaining': remaining,
                    'unresolved_candidates': len(remaining), 'status': 'solved',
                    'evidence': {}, 'edges': [], 'checked_proofs': len(proofs), 'needs_expansion': False}
        nodes = [node(set())]
        def edge(u, v, lit):
            nodes[u]['status'] = 'open'
            nodes[u]['evidence'][str(lit - 1)] = [lit]
            nodes[u]['edges'].append({'target': v, 'proof': lit - 1, 'deduced': [lit],
                                      'implied': [], 'active': True})
        for stage in range(layers):
            u = len(nodes) - 1
            a, b = stage * 2 + 1, stage * 2 + 2
            known = set(range(1, a))
            nodes.extend([node(known | {a}), node(known | {b}), node(known | {a, b})])
            edge(u, u + 1, a)
            edge(u, u + 2, b)
            edge(u + 1, u + 3, b)
            edge(u + 2, u + 3, a)
        graph = {'format': 'demystify-solve-graph', 'version': 1, 'fixed_point': True,
                 'root': 0, 'nodes': nodes, 'proofs': proofs}
        result = analyse(graph)
        self.assertEqual(result['state_routes'], str(2 ** layers))
        self.assertEqual(result['release']['chains']['min'], layers)
        self.assertEqual(result['moves']['min'], 2 * layers)
        self.assertEqual(result['chokes']['narrow_routes'][0]['encounters']['min'], layers)
        self.assertEqual(result['chokes']['narrow_routes'][0]['longest_streak']['max'], 1)

    def test_against_exhaustive_routes(self):
        for seed in range(25):
            rng = random.Random(seed)
            n = 5
            bases = [rng.randint(2, 6) for _ in range(n)]
            gains = [[rng.randint(0, 3) for _ in range(n)] for _ in range(n)]
            def cost(known, lit):
                return max(1, bases[lit - 1] - sum(gains[lit - 1][x - 1] for x in known))
            story = Story(fixture(n, cost))
            paths = list(routes(story))
            result = story.analyse()
            self.assertEqual(int(result['state_routes']), len(paths))
            depths = [reference_depth(story, path) for path in paths]
            self.assertEqual((result['release']['chains']['min'], result['release']['chains']['max']),
                             (min(depths), max(depths)), seed)
            for row in result['hard_moves']:
                flags = [[story.by_key[e].size >= row['size_at_least'] for e in path] for path in paths]
                self.assertEqual((row['min'], row['max']), (min(map(sum, flags)), max(map(sum, flags))))
                self.assertEqual((row['longest_streak']['min'], row['longest_streak']['max']),
                                 (min(map(longest_run, flags)), max(map(longest_run, flags))))
            choke_peaks = [max((story.by_key[e].size for e in path
                                if story.width[story.by_key[e].source] == 1), default=0)
                           for path in paths]
            self.assertEqual((result['chokes']['peak_mus']['min'], result['chokes']['peak_mus']['max']),
                             (min(choke_peaks), max(choke_peaks)))
            for row in result['chokes']['by_threshold']:
                counts = [sum(story.width[story.by_key[e].source] == 1 and
                              story.by_key[e].size >= row['size_at_least'] for e in path)
                          for path in paths]
                self.assertEqual((row['min'], row['max']), (min(counts), max(counts)))
            common = set.intersection(*(set(story.by_key[e].source for e in path) for path in paths))
            self.assertEqual({s['state'] for s in result['chokes']['states'] if s['unavoidable_state']},
                             {u for u in common if story.width[u] == 1})
            for row in result['chokes']['narrow_routes']:
                flags = [[story.width[story.by_key[e].source] <= row['width_at_most'] for e in path] for path in paths]
                self.assertEqual((row['longest_streak']['min'], row['longest_streak']['max']),
                                 (min(map(longest_run, flags)), max(map(longest_run, flags))))
            for witness in result['witnesses'].values():
                path = tuple((s['state'], s['edge']) for s in witness['steps'])
                self.assertEqual(reference_depth(story, path), witness['release']['depth'])


class CorpusTests(unittest.TestCase):
    def test_database_roundtrip_idempotence_and_stale_hash_exclusion(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / 'test.graph.json'
            path.write_text(json.dumps(fixture(3, lambda s, lit: lit)))
            sha = hashlib.sha256(path.read_bytes()).hexdigest()
            with sqlite3.connect(root / 'corpus.sqlite') as db:
                db.executescript('''
                    CREATE TABLE puzzles(id TEXT PRIMARY KEY,game TEXT,source_tier INTEGER,
                                         source_pack TEXT,board_area INTEGER);
                    CREATE TABLE runs(puzzle_id TEXT,phase TEXT,status TEXT,archive_path TEXT,
                                      archive_sha256 TEXT,viewer_path TEXT,PRIMARY KEY(puzzle_id,phase));
                ''')
                db.execute("INSERT INTO puzzles VALUES ('p','game',1,'pack',3)")
                db.execute("INSERT INTO runs VALUES ('p','baseline','complete',?,?,NULL)", (path.name, sha))
                db.execute("INSERT INTO puzzles VALUES ('unfinished','game',1,'pack',3)")
                db.execute("INSERT INTO runs VALUES ('unfinished','baseline','checkpoint',NULL,NULL,NULL)")
            args = SimpleNamespace(corpus=root, phase=None, ids=None, max_labels=100000)
            for _ in range(2):
                with contextlib.redirect_stdout(io.StringIO()):
                    driver.corpus(args)
            with sqlite3.connect(root / 'corpus.sqlite') as db:
                self.assertEqual(db.execute('SELECT count(*) FROM current_story_statistics').fetchone()[0], 1)
                self.assertEqual(db.execute('SELECT count(*) FROM story_thresholds').fetchone()[0], 3)
                self.assertEqual(db.execute('SELECT count(*) FROM story_pacing').fetchone()[0], 3)
                self.assertEqual(db.execute('SELECT count(*) FROM story_flow').fetchone()[0], 1)
                self.assertEqual(db.execute('SELECT expected_hard,tail_min,tail_max FROM story_pacing '
                                            'WHERE threshold=2').fetchone(), (2.0, 0, 0))
                self.assertEqual(db.execute("SELECT status FROM story_runs WHERE puzzle_id='unfinished'").fetchone()[0], 'excluded')
                rel, recorded = db.execute("SELECT statistics_path,statistics_sha256 FROM story_runs WHERE puzzle_id='p'").fetchone()
                self.assertEqual(hashlib.sha256((root / rel).read_bytes()).hexdigest(), recorded)
                db.execute("UPDATE runs SET archive_sha256='changed' WHERE puzzle_id='p'")
                self.assertEqual(db.execute('SELECT count(*) FROM current_story_statistics').fetchone()[0], 0)
            with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(SystemExit):
                driver.corpus(args)
            with sqlite3.connect(root / 'corpus.sqlite') as db:
                self.assertEqual(db.execute('SELECT count(*) FROM story_thresholds').fetchone()[0], 0)
                for table in ('story_flow', 'story_pacing', 'story_availability', 'story_independence', 'story_profiles'):
                    self.assertEqual(db.execute('SELECT count(*) FROM ' + table).fetchone()[0], 0)
                self.assertEqual(db.execute("SELECT status FROM story_runs WHERE puzzle_id='p'").fetchone()[0], 'error')
                self.assertEqual(db.execute('PRAGMA foreign_key_check').fetchall(), [])


if __name__ == '__main__':
    unittest.main()
