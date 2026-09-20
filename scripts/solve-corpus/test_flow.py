"""Independent route enumeration and metadata cases for flow measurements."""

import math
import random
import unittest

from flow import Flow
from story import Story, analyse
from test_story import fixture, routes


def metadata(graph, family=lambda lit: 'rule'):
    graph['puzzle'] = {
        'eprime': {'vars': ['cell']},
        'invlitmap': {str(lit): [{'varval': {'var': {'name': 'cell', 'indices': [lit]}, 'val': 1},
                                 'equal': True}]
                      for lit in graph['nodes'][graph['root']]['remaining']},
        'family_of': {}, 'constraint_metadata': {},
    }
    for u in graph['nodes']:
        for raw, literals in u['evidence'].items():
            for c in graph['proofs'][int(raw)]['constraints']:
                graph['puzzle']['family_of'][str(c)] = family(literals[0])
                graph['puzzle']['constraint_metadata'][str(c)] = {
                    'targets': [{'kind': 'cells', 'keys': ['0,0', '2,1']}]}
    return graph


def interval(row):
    return (row['min'], row['max'])


class FlowTests(unittest.TestCase):
    def test_breakthrough_tail_and_probability(self):
        def cost(s, lit):
            return {1: 5, 2: 1 if {1, 3} <= s else 5, 3: 1 if 1 in s else 6}[lit]
        graph = metadata(fixture(3, cost))
        result = analyse(graph)
        row = result['flow']['thresholds'][4]
        self.assertEqual(interval(row['following_easy_moves']), (0, 2))
        self.assertEqual(interval(row['between_hard_easy_moves']), (0, 0))
        tail = row['routine_tail_moves']['with_hard_move']
        self.assertEqual(interval(tail), (1, 2))
        self.assertAlmostEqual(tail['expected_conditional'], 1.5)
        self.assertAlmostEqual(row['expected_hard_moves'], 1.5)
        self.assertAlmostEqual(row['probability_any_hard_move'], 1)
        self.assertEqual(interval(row['routine_tail_settled_variables']['with_hard_move']), (1, 2))
        self.assertEqual(result['flow']['metadata']['variable_mapping'], 'complete')

    def test_queue_promotions_do_not_count_as_newly_easy(self):
        result = analyse(fixture(3, lambda s, lit: lit))
        self.assertTrue(all(p['newly_easy_literals'] == 0
                            for e in result['flow']['edges'] for p in e['payoff']))
        self.assertTrue(all(r['routine_tail_moves']['with_hard_move']['max'] == 0
                            for r in result['flow']['thresholds']))

    def test_unknown_proof_availability_is_visible(self):
        def cost(s, lit):
            return 3 if lit == 1 else 1 if 1 in s else None
        row = analyse(fixture(2, cost))['flow']['thresholds'][2]
        self.assertEqual(interval(row['newly_easy_literals']), (1, 1))
        self.assertEqual(interval(row['previously_unexplained_literals']), (1, 1))

    def test_diamonds_and_cost_changing_commutation_are_distinct(self):
        result = analyse(fixture(3, lambda s, lit: 2))['flow']['independence']
        root = result['states'][0]
        self.assertEqual(root['union_diamonds'], 3)
        self.assertEqual(root['stable_disjoint_pairs'], 3)
        self.assertEqual(root['local_group_count'], 3)
        self.assertEqual(interval(result['peak_local_groups']), (3, 3))
        self.assertEqual(result['union_diamonds'], 6)
        # Either choice makes the other cheaper: a diamond, but not unchanged-cost independence.
        changed = analyse(fixture(2, lambda s, lit: 1 if s else 2))['flow']['independence']['states'][0]
        self.assertEqual(changed['union_diamonds'], 1)
        self.assertEqual(changed['stable_disjoint_pairs'], 0)
        self.assertEqual(changed['local_group_count'], 1)
        linear = analyse(fixture(3, lambda s, lit: lit))['flow']['independence']
        self.assertEqual(linear['union_diamonds'], 0)
        self.assertEqual(interval(linear['peak_local_groups']), (1, 1))

    def test_missing_geometry_is_not_inferred_from_flat_variable_indices(self):
        graph = metadata(fixture(2, lambda s, lit: 1))
        initial = analyse(graph)['flow']['proof_profiles']
        self.assertEqual(initial['proofs'][0]['target_bounding_box']['span'], [2, 1])
        for row in graph['puzzle']['constraint_metadata'].values():
            row['targets'] = [{'kind': 'cell', 'key': '17'}]
        result = analyse(graph)['flow']
        self.assertTrue(all(p['target_bounding_box'] is None for p in result['proof_profiles']['proofs']))
        graph['puzzle']['invlitmap'].pop('2')
        partial = analyse(graph)['flow']
        self.assertEqual(partial['metadata']['variable_mapping'], 'partial')
        self.assertIsNone(partial['availability'][0]['cheapest_variables'])
        self.assertNotIn('routine_tail_settled_variables', partial['thresholds'][0])

    def test_profile_repetition_and_equivalent_explanations(self):
        graph = metadata(fixture(3, lambda s, lit: 1, duplicate=True), lambda lit: 'A' if lit < 3 else 'B')
        result = analyse(graph)['flow']
        self.assertAlmostEqual(result['expected_moves'], 3)
        profiles = result['proof_profiles']
        self.assertAlmostEqual(profiles['expected_distinct_profiles'], 2)
        self.assertAlmostEqual(profiles['expected_repeated_profile_uses'], 1)
        self.assertTrue(all(r['unavoidable_profile'] for r in profiles['profiles']))
        graph = metadata(fixture(1, lambda s, lit: 1, duplicate=True))
        c = graph['proofs'][1]['constraints'][0]
        graph['puzzle']['family_of'][str(c)] = 'different'
        profiles = analyse(graph)['flow']['proof_profiles']
        self.assertAlmostEqual(profiles['expected_distinct_profiles'], 1)
        self.assertTrue(all(r['probability_seen'] == 0.5 for r in profiles['profiles']))
        self.assertTrue(all(not r['unavoidable_profile'] for r in profiles['profiles']))
        graph['puzzle']['family_of'].pop(str(c))
        profiles = analyse(graph)['flow']['proof_profiles']
        self.assertEqual(profiles['status'], 'partial')
        self.assertIsNone(profiles['expected_distinct_profiles'])

    def test_equivalent_successors_choose_batches_before_profiles(self):
        graph = metadata(fixture(2, lambda s, lit: 1, duplicate=True), lambda lit: str(lit))
        terminal = next(i for i, n in enumerate(graph['nodes']) if n['status'] == 'solved')
        for edge in graph['nodes'][0]['edges']:
            edge['target'] = terminal
        c = graph['proofs'][-1]['constraints'][0]
        graph['puzzle']['family_of'][str(c)] = 'alternative'
        result = analyse(graph)
        self.assertEqual(result['state_routes'], '1')
        flow = result['flow']
        self.assertAlmostEqual(flow['expected_moves'], 1)
        self.assertEqual([e['probability_given_state'] for e in flow['edges']], [0.5, 0.5])
        rows = {tuple(r['family_multiset']): r for r in flow['proof_profiles']['profiles']}
        self.assertEqual(rows[('1',)]['probability_seen'], 0.25)
        self.assertEqual(rows[('alternative',)]['probability_seen'], 0.25)
        self.assertEqual(rows[('2',)]['probability_seen'], 0.5)

    def test_automatic_closure_is_separate_and_counts_settlements(self):
        graph = metadata(fixture(1, lambda s, lit: 1))
        graph['nodes'][1]['known'].append(50)
        graph['nodes'][0]['edges'][0]['implied'] = [50]
        graph['puzzle']['invlitmap']['50'] = [{'varval': {'var': {'name': 'cell', 'indices': [50]}, 'val': 1},
                                             'equal': True}]
        flow = analyse(graph)['flow']
        self.assertEqual(flow['edges'][0]['progress'], {
            'explicit_literals': 1, 'automatic_literals': 1, 'gained_literals': 2,
            'affected_variables': 2, 'settled_variables': 2})
        graph['puzzle']['invlitmap'].pop('50')
        partial = analyse(graph)['flow']
        self.assertEqual(partial['metadata']['variable_mapping'], 'partial')
        self.assertIsNone(partial['edges'][0]['progress']['settled_variables'])

    def test_solved_root_and_no_hard_routes_are_not_zero_length_hard_events(self):
        flow = analyse(fixture(0, lambda s, lit: 1))['flow']
        self.assertEqual(flow['thresholds'], [])
        self.assertEqual(flow['expected_moves'], 0)
        self.assertEqual(flow['proof_profiles']['expected_distinct_profiles'], 0)
        # Exercise a threshold that no route encounters directly.
        flow = Flow(Story(fixture(2, lambda s, lit: 1)))
        tail = flow.tail(2, lambda m: 1)
        self.assertEqual(tail['with_hard_move']['status'], 'not_applicable')
        self.assertEqual(tail['with_hard_move']['probability'], 0)
        self.assertIsNone(tail['with_hard_move']['expected_conditional'])
        self.assertEqual(interval(tail['without_hard_move']), (2, 2))

    def test_all_new_bounds_and_probabilities_against_exhaustive_routes(self):
        for seed in range(20):
            rng = random.Random(seed)
            bases = [rng.randint(2, 6) for _ in range(5)]
            gains = [[rng.randint(0, 3) for _ in range(5)] for _ in range(5)]
            graph = metadata(fixture(5, lambda known, lit: max(
                1, bases[lit - 1] - sum(gains[lit - 1][x - 1] for x in known))))
            story = Story(graph)
            model = Flow(story)
            result = story.analyse()
            paths = list(routes(story))
            probabilities = [math.prod(model.weights[e] for e in path) for path in paths]
            self.assertAlmostEqual(sum(probabilities), 1)
            self.assertAlmostEqual(result['flow']['expected_moves'],
                                   sum(p * len(path) for p, path in zip(probabilities, paths)))
            self.assertAlmostEqual(result['flow']['proof_profiles']['expected_distinct_profiles'],
                                   sum(p * len({story.by_key[e].size for e in path})
                                       for p, path in zip(probabilities, paths)))
            for row in result['flow']['thresholds']:
                k = row['hard_threshold']
                tails, without, stretches, intervals, new = [], [], [], [], []
                mean_hard, chance_choke, mean_tail, prob_hard = 0, 0, 0, 0
                for p, path in zip(probabilities, paths):
                    costs = [story.by_key[e].size for e in path]
                    hard = [i for i, cost in enumerate(costs) if cost >= k]
                    mean_hard += p * len(hard)
                    chance_choke += p * any(story.width[story.by_key[path[i]].source] == 1 for i in hard)
                    if hard:
                        tails.append(len(path) - 1 - hard[-1])
                        mean_tail += p * tails[-1]
                        prob_hard += p
                    else:
                        without.append(len(path))
                    for i in hard:
                        end = i + 1
                        while end < len(path) and costs[end] < k:
                            end += 1
                        stretches.append(end - i - 1)
                        if end < len(path):
                            intervals.append(end - i - 1)
                        move = story.by_key[path[i]]
                        new.append(sum(c < k and story.best[move.source].get(lit, float('inf')) >= k
                                       for lit, c in story.best[move.target].items()))
                for expected, actual in [(tails, row['routine_tail_moves']['with_hard_move']),
                                         (without, row['routine_tail_moves']['without_hard_move']),
                                         (stretches, row['following_easy_moves']),
                                         (intervals, row['between_hard_easy_moves']),
                                         (new, row['newly_easy_literals'])]:
                    self.assertEqual(interval(actual), (min(expected), max(expected)) if expected else (None, None), seed)
                self.assertAlmostEqual(row['expected_hard_moves'], mean_hard)
                self.assertAlmostEqual(row['probability_any_hard_choke'], chance_choke)
                tail = row['routine_tail_moves']['with_hard_move']
                self.assertAlmostEqual(tail['probability'], prob_hard)
                if prob_hard:
                    self.assertAlmostEqual(tail['expected_conditional'], mean_tail / prob_hard)
                # Independently evaluate the full route and marked event behind each bound.
                for field in ('following_easy_moves', 'between_hard_easy_moves', 'newly_easy_literals'):
                    bound = row[field]
                    if bound['status'] != 'complete':
                        continue
                    for which in ('min', 'max'):
                        witness = result['witnesses'][bound[which + '_route']]
                        i = bound[which + '_event_step']
                        step = witness['steps'][i]
                        if field == 'newly_easy_literals':
                            value = sum(c < k and story.best[step['state']].get(lit, float('inf')) >= k
                                        for lit, c in story.best[step['target']].items())
                        else:
                            end = i + 1
                            while end < len(witness['steps']) and witness['steps'][end]['mus_size'] < k:
                                end += 1
                            value = end - i - 1
                            if field == 'between_hard_easy_moves':
                                self.assertLess(end, len(witness['steps']))
                        self.assertEqual(value, bound[which])
            # Nearest reconvergence must be the first common state on every continuation.
            for row in result['flow']['independence']['states']:
                u = row['state']
                if story.width[u] <= 1:
                    continue
                continuations = list(routes(story, u))
                common = set.intersection(*(set(story.by_key[e].target for e in p) for p in continuations))
                first = min(common, key=lambda v: len(story.nodes[v]['known'])) if common else None
                self.assertEqual(row['first_common_state'], first)


if __name__ == '__main__':
    unittest.main()
