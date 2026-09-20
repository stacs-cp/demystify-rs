"""Pacing, local commutation and metadata proxies on an existing cheapest DAG.

No SAT calls, geometry guesses, or identification of human reasoning strategies.
"""

from collections import Counter, defaultdict
from itertools import combinations
import json


class Flow:
    def __init__(self, story):
        self.s = story
        self.known = {u: frozenset(story.nodes[u]['known']) for u in story.order}
        self.successors = {u: sorted({m.target for m in story.moves[u]}) for u in story.order}
        self.prefix = {story.root: ()}
        self.probability = {u: 0.0 for u in story.order}
        self.probability[story.root] = 1.0
        self.weights = {}
        for u in story.order:
            batches = Counter(m.target for m in story.moves[u])
            for m in story.moves[u]:
                # A proof duplicate never changes state/batch selection.
                self.weights[m.key] = 1.0 / (len(batches) * batches[m.target])
                self.probability[m.target] += self.probability[u] * self.weights[m.key]
                self.prefix.setdefault(m.target, self.prefix[u] + (m.key,))
        self.puzzle = story.graph.get('puzzle', {})
        primary = set(self.puzzle.get('eprime', {}).get('vars', []))
        self.variables, self.assignments = {}, {}
        for raw, aliases in self.puzzle.get('invlitmap', {}).items():
            vs, assigned = set(), set()
            for alias in aliases:
                var = alias['varval']['var']
                if var['name'] in primary:
                    key = (var['name'], tuple(var['indices']))
                    vs.add(key)
                    if alias['equal']:
                        assigned.add(key)
            if vs:
                self.variables[int(raw)] = vs
                self.assignments[int(raw)] = assigned
        needed = {lit for u in story.order for lit in story.nodes[u]['remaining']}
        mapped = needed & self.variables.keys()
        # Automatic-only facts may never occur in a node's `remaining` list.
        # Without an alias entry we cannot tell whether one settles a primary
        # variable, so suppress variable totals rather than silently undercount.
        self.unknown_known = {lit for known in self.known.values() for lit in known
                              if str(lit) not in self.puzzle.get('invlitmap', {})}
        self.variable_status = ('complete' if primary and mapped == needed and not self.unknown_known else
                                'partial' if mapped else 'unavailable')
        self.settled = {u: self.vars_for(self.known[u], assignments=True) for u in story.order}
        self.progress = {}
        for m in story.by_key.values():
            gained = self.known[m.target] - self.known[m.source]
            explicit = set(m.deduced)
            if not explicit <= gained:
                raise ValueError('Explicit deductions are missing from successor knowledge')
            self.progress[m.key] = {
                'explicit_literals': len(explicit),
                'automatic_literals': len(gained - explicit),
                'gained_literals': len(gained),
                'affected_variables': self.variable_count(gained),
                'settled_variables': (len(self.settled[m.target] - self.settled[m.source])
                                      if self.variable_status == 'complete' else None),
            }

    def vars_for(self, literals, assignments=False):
        mapping = self.assignments if assignments else self.variables
        return set().union(*(mapping.get(lit, set()) for lit in literals)) if literals else set()

    def variable_count(self, literals):
        return len(self.vars_for(literals)) if self.variable_status == 'complete' else None

    def expected(self, weight):
        return sum(self.probability[u] * self.weights[m.key] * weight(m)
                   for u in self.s.order for m in self.s.moves[u])

    def chance(self, predicate):
        """Probability of at least one matching move, without double counting."""
        values = {}
        for u in reversed(self.s.order):
            values[u] = sum(self.weights[m.key] * (1.0 if predicate(m) else values[m.target])
                            for m in self.s.moves[u])
        return values[self.s.root]

    def bound(self, candidates):
        """Bounds over (value, complete route, optional event step) records."""
        if not candidates:
            return {'status': 'not_applicable', 'min': None, 'max': None}
        out = {'status': 'complete'}
        for name, select in [('min', min), ('max', max)]:
            value, route, event = select(candidates, key=lambda x: x[0])
            out[name] = value
            out[name + '_route'] = self.s.intern(route)
            if event is not None:
                out[name + '_event_step'] = event
        out['gap'] = out['max'] - out['min']
        return out

    def tail(self, k, weight):
        """Terminal cheap suffix, partitioned by whether any hard move occurs.

        Keep min/max witnesses and probability-weighted first moments for both
        partitions. A cheap prefix before a later hard step contributes nothing.
        """
        bounds, masses = {}, {}
        for u in reversed(self.s.order):
            candidates = defaultdict(list)
            moments = defaultdict(lambda: [0.0, 0.0])
            if not self.s.moves[u]:
                candidates[False] = [(0, (), None)]
                moments[False] = [1.0, 0.0]
            for m in self.s.moves[u]:
                for child_hard, extrema in bounds[m.target].items():
                    has_hard = child_hard or m.size >= k
                    increment = weight(m) if m.size < k and not child_hard else 0
                    for value, path, _ in extrema:
                        candidates[has_hard].append((value + increment, (m.key,) + path, None))
                    p, moment = masses[m.target][child_hard]
                    q = self.weights[m.key]
                    moments[has_hard][0] += q * p
                    moments[has_hard][1] += q * (moment + increment * p)
            bounds[u] = {flag: [min(rows, key=lambda x: x[0]), max(rows, key=lambda x: x[0])]
                         for flag, rows in candidates.items()}
            masses[u] = moments
        result = {}
        for flag, name in [(True, 'with_hard_move'), (False, 'without_hard_move')]:
            result[name] = self.bound(bounds[self.s.root].get(flag, []))
            p, moment = masses[self.s.root].get(flag, [0.0, 0.0])
            result[name]['probability'] = p
            result[name]['expected_conditional'] = moment / p if p else None
        return result

    def segments(self, k, weight):
        """Cheap continuations ending immediately before a hard move or at solve."""
        bounds = {}
        for u in reversed(self.s.order):
            if not self.s.moves[u] or self.s.minimum[u] >= k:
                stop = 'hard' if self.s.moves[u] else 'solved'
                bounds[u] = {stop: [(0, (), u), (0, (), u)]}
                continue
            candidates = defaultdict(list)
            for m in self.s.moves[u]:
                for stop, rows in bounds[m.target].items():
                    for value, path, endpoint in rows:
                        candidates[stop].append((weight(m) + value, (m.key,) + path, endpoint))
            bounds[u] = {stop: [min(rows, key=lambda x: x[0]), max(rows, key=lambda x: x[0])]
                         for stop, rows in candidates.items()}
        return bounds

    def event_bounds(self, moves, value):
        return self.bound([(value(m), self.s.extend(self.prefix[m.source] + (m.key,), m.target),
                            len(self.prefix[m.source])) for m in moves])

    def segment_bounds(self, moves, segments, stop=None):
        candidates = []
        for m in moves:
            for kind, rows in segments[m.target].items():
                if stop is not None and kind != stop:
                    continue
                for value, path, endpoint in rows:
                    prefix = self.prefix[m.source] + (m.key,)
                    candidates.append((value, self.s.extend(prefix + path, endpoint), len(prefix) - 1))
        return self.bound(candidates)

    def pacing(self):
        s = self.s
        thresholds, edges = [], {m.key: [] for m in s.by_key.values()}
        for k in range(1, max(s.sizes, default=0) + 1):
            hard = [m for m in s.by_key.values() if m.size >= k]
            if not hard:
                break
            counts, old_counts, unknown_counts = {}, {}, {}
            segments = self.segments(k, lambda m: 1)
            for m in hard:
                newly = [r['literal'] for r in s.transitions[m.key]['released']
                         if r['to_size'] < k and (r['from_size'] is None or r['from_size'] >= k)]
                old = [lit for lit, cost in s.best[m.target].items()
                       if cost < k and s.best[m.source].get(lit, float('inf')) < k]
                unknown = [r['literal'] for r in s.transitions[m.key]['released']
                           if r['to_size'] < k and r['from_size'] is None]
                counts[m.key], old_counts[m.key], unknown_counts[m.key] = len(newly), len(old), len(unknown)
                lengths = [r[0] for rows in segments[m.target].values() for r in rows]
                edges[m.key].append({
                    'hard_threshold': k, 'newly_easy_literals': len(newly),
                    'newly_easy_variables': self.variable_count(newly),
                    'previously_easy_remaining_literals': len(old),
                    'previously_unexplained_literals': len(unknown),
                    'following_easy_moves_min': min(lengths), 'following_easy_moves_max': max(lengths),
                })
            row = {
                'hard_threshold': k, 'easy_means': f'MUS size < {k}',
                'hard_events': len(hard),
                'newly_easy_literals': self.event_bounds(hard, lambda m: counts[m.key]),
                'previously_easy_remaining_literals': self.event_bounds(hard, lambda m: old_counts[m.key]),
                'previously_unexplained_literals': self.event_bounds(hard, lambda m: unknown_counts[m.key]),
                'following_easy_moves': self.segment_bounds(hard, segments),
                'between_hard_easy_moves': self.segment_bounds(hard, segments, stop='hard'),
                'routine_tail_moves': self.tail(k, lambda m: 1),
                'routine_tail_gained_literals': self.tail(k, lambda m: self.progress[m.key]['gained_literals']),
                'expected_hard_moves': self.expected(lambda m: int(m.size >= k)),
                'probability_any_hard_move': self.chance(lambda m: m.size >= k),
                'probability_any_hard_choke': self.chance(lambda m: m.size >= k and s.width[m.source] == 1),
                'expected_newly_easy_literals': self.expected(lambda m: counts.get(m.key, 0)),
            }
            if self.variable_status == 'complete':
                settled = lambda m: self.progress[m.key]['settled_variables']
                row['following_easy_settled_variables'] = self.segment_bounds(hard, self.segments(k, settled))
                row['routine_tail_settled_variables'] = self.tail(k, settled)
            thresholds.append(row)
        return thresholds, edges

    def independence(self):
        s = self.s
        state_for = {known: u for u, known in self.known.items()}
        if len(state_for) != len(self.known):
            raise ValueError('Distinct active states have identical knowledge')
        successor_sets = {u: set(vs) for u, vs in self.successors.items()}
        # All postdominators, using integer bitsets and a topological numbering.
        positions = {u: i for i, u in enumerate(s.order)}
        post = {}
        for u in reversed(s.order):
            children = self.successors[u]
            common = post[children[0]] if children else 0
            for v in children[1:]:
                common &= post[v]
            post[u] = (1 << positions[u]) | common
        rows, by_state = [], {}
        for u in s.order:
            children = self.successors[u]
            pairs = []
            stable = set()
            for a, b in combinations(children, 2):
                union = self.known[a] | self.known[b]
                join = state_for.get(union)
                if join not in successor_sets[a] or join not in successor_sets[b]:
                    continue
                disjoint = not ((self.known[a] - self.known[u]) & (self.known[b] - self.known[u]))
                same_cost = s.minimum[a] == s.minimum[b] == s.minimum[u]
                pairs.append({'first_targets': [a, b], 'join': join,
                              'disjoint_gains': disjoint, 'unchanged_cost': same_cost})
                if disjoint and same_cost:
                    stable.add((a, b))
            # Connected components of NON-commuting choices: choices in different
            # components commute pairwise locally. This is not a decomposition
            # of the entire future deduction problem.
            pending, groups = set(children), []
            while pending:
                group, queue = [], [min(pending)]
                pending.remove(queue[0])
                for a in queue:
                    group.append(a)
                    neighbours = [b for b in sorted(pending) if tuple(sorted((a, b))) not in stable]
                    pending.difference_update(neighbours)
                    queue.extend(neighbours)
                groups.append(sorted(group))
            common = post[u] & ~(1 << positions[u])
            join = s.order[(common & -common).bit_length() - 1] if common and len(children) > 1 else None
            row = {'state': u, 'width': len(children), 'pairs_tested': len(children) * (len(children) - 1) // 2,
                   'union_diamonds': len(pairs), 'stable_disjoint_pairs': len(stable),
                   'local_groups': groups, 'local_group_count': len(groups),
                   'first_common_state': join, 'diamond_examples': pairs[:8],
                   'diamond_examples_truncated': len(pairs) > 8}
            rows.append(row)
            by_state[u] = row
        return {
            'scope': 'Two-step outcome commutation and local groups; not global causal independence',
            'states': rows,
            'union_diamonds': sum(r['union_diamonds'] for r in rows),
            'stable_disjoint_pairs': sum(r['stable_disjoint_pairs'] for r in rows),
            'branch_states': sum(r['width'] > 1 for r in rows),
            'reconverging_branch_states': sum(r['first_common_state'] is not None for r in rows),
            'multi_group_moves': s.extrema(lambda m: int(by_state[m.source]['local_group_count'] > 1)),
            'peak_local_groups': s.extrema(lambda m: by_state[m.source]['local_group_count'], peak=True),
            'opening_moves': s.extrema(lambda m: int(s.width[m.target] > s.width[m.source])),
            'closing_moves': s.extrema(lambda m: int(s.width[m.target] < s.width[m.source])),
            'expected_multi_group_moves': self.expected(lambda m: int(by_state[m.source]['local_group_count'] > 1)),
        }

    def profiles(self):
        """Coarse family multisets. Preserve names exactly; never strip digits."""
        s = self.s
        families = self.puzzle.get('family_of', {})
        metadata = self.puzzle.get('constraint_metadata', {})
        used = sorted({p for m in s.by_key.values() for p in m.proofs})
        signatures, proof_rows = {}, []
        for p in used:
            constraints = s.graph['proofs'][p]['constraints']
            names = [families.get(str(c)) for c in constraints]
            signature = tuple(sorted(names)) if all(names) else None
            signatures[p] = signature
            # Display targets may identify cells without giving coordinates.
            # Retain them, but do not infer coordinates from flat indices.
            targets, complete = set(), True
            for c in constraints:
                if str(c) not in metadata:
                    complete = False
                for target in metadata.get(str(c), {}).get('targets', []):
                    targets.add(json.dumps(target, sort_keys=True, separators=(',', ':')))
            cells = set()
            coordinate_complete = complete
            for raw in targets:
                target = json.loads(raw)
                if target.get('kind') == 'cells':
                    keys = target.get('keys', [])
                elif target.get('kind') == 'cell':
                    keys = [target.get('key')]
                else:
                    coordinate_complete = False
                    continue
                if not keys:
                    coordinate_complete = False
                for key in keys:
                    parts = str(key).split(',')
                    try:
                        if len(parts) != 2:
                            raise ValueError()
                        cells.add(tuple(map(int, parts)))
                    except (TypeError, ValueError):
                        coordinate_complete = False
            box = None
            if coordinate_complete and cells:
                xs, ys = zip(*cells)
                box = {'min': [min(xs), min(ys)], 'max': [max(xs), max(ys)],
                       'span': [max(xs) - min(xs), max(ys) - min(ys)]}
            proof_rows.append({'proof': p, 'family_multiset': list(signature) if signature else None,
                               'targets': [json.loads(t) for t in sorted(targets)],
                               'target_metadata_complete': complete,
                               'coordinate_coverage': ('complete' if box else 'partial' if cells else 'unavailable'),
                               'target_coordinate_count': len(cells), 'target_bounding_box': box})
        profile_keys = sorted({sig for sig in signatures.values() if sig is not None})
        profile_ids = {sig: i for i, sig in enumerate(profile_keys)}
        options = {m.key: sorted({profile_ids[signatures[p]] for p in m.proofs if signatures[p] is not None})
                   for m in s.by_key.values()}
        complete = all(signatures[p] is not None for p in used)
        result = {
            'status': 'complete' if complete else 'partial' if profile_keys else 'unavailable',
            'meaning': 'Multiset of saved constraint family names; a coarse proxy, not a human strategy',
            'proofs_with_family_metadata': sum(signatures[p] is not None for p in used),
            'active_proofs': len(used), 'proofs': proof_rows, 'profiles': [],
            'expected_distinct_profiles': None,
        }
        for identifier, signature in enumerate(profile_keys):
            row = {'profile': identifier, 'family_multiset': list(signature), 'mus_size': len(signature)}
            if complete:
                avoid, possible_avoid = {}, {}
                expected_uses = 0.0
                for u in reversed(s.order):
                    avoid[u] = 1.0 if not s.moves[u] else 0.0
                    possible_avoid[u] = not s.moves[u]
                    for m in s.moves[u]:
                        choices = options[m.key]
                        use = 1.0 / len(choices) if identifier in choices else 0.0
                        avoid[u] += self.weights[m.key] * (1.0 - use) * avoid[m.target]
                        possible_avoid[u] |= use < 1.0 and possible_avoid[m.target]
                        expected_uses += self.probability[u] * self.weights[m.key] * use
                row.update(probability_seen=max(0.0, min(1.0, 1.0 - avoid[s.root])),
                           expected_uses=expected_uses,
                           unavoidable_profile=not possible_avoid[s.root])
            result['profiles'].append(row)
        if complete:
            result['expected_distinct_profiles'] = sum(r['probability_seen'] for r in result['profiles'])
            result['expected_repeated_profile_uses'] = max(
                0.0, self.expected(lambda m: 1) - result['expected_distinct_profiles'])
        return result, options

    def analyse(self):
        s = self.s
        pacing, payoff = self.pacing()
        independence = self.independence()
        profiles, options = self.profiles()
        availability = []
        for u in s.order:
            remaining = s.nodes[u]['remaining']
            cheapest = [lit for lit, cost in s.best[u].items() if cost == s.minimum[u]]
            unresolved_vars = self.vars_for(remaining) - self.settled[u]
            cheapest_vars = self.vars_for(cheapest) & unresolved_vars
            availability.append({
                'state': u, 'remaining_literals': len(remaining), 'cheapest_literals': len(cheapest),
                'cheapest_literal_fraction': len(cheapest) / len(remaining) if remaining else None,
                'unresolved_variables': len(unresolved_vars) if self.variable_status == 'complete' else None,
                'cheapest_variables': len(cheapest_vars) if self.variable_status == 'complete' else None,
                'cheapest_variable_fraction': (len(cheapest_vars) / len(unresolved_vars)
                                               if unresolved_vars and self.variable_status == 'complete' else None),
                'visit_probability': self.probability[u],
            })
        edge_rows = [{'state': m.source, 'edge': m.edge, 'target': m.target, 'mus_size': m.size,
                      'probability_given_state': self.weights[m.key],
                      'profile_options': options[m.key],
                      'progress': self.progress[m.key], 'payoff': payoff[m.key]}
                     for m in s.by_key.values()]
        return {
            'version': 1,
            'random_policy': ('Uniform distinct cheapest successor; uniform distinct deduction batch '
                              'within successor; for novelty, uniform distinct available family profile '
                              'within batch. Floating-point probabilities; not uniform complete routes.'),
            'metadata': {'variable_mapping': self.variable_status,
                         'known_literals_without_aliases': len(self.unknown_known),
                         'remaining_literals': len({lit for u in s.order for lit in s.nodes[u]['remaining']}),
                         'mapped_remaining_literals': len({lit for u in s.order for lit in s.nodes[u]['remaining']
                                                           if lit in self.variables}),
                         'unit': 'Primary puzzle variable; not assumed to equal one board cell'},
            'thresholds': pacing, 'edges': edge_rows, 'availability': availability,
            'independence': independence, 'proof_profiles': profiles,
            'expected_moves': self.expected(lambda m: 1),
            'progress': {key: s.extrema(lambda m: self.progress[m.key][key])
                         for key in ('explicit_literals', 'automatic_literals', 'gained_literals')},
        }


def analyse(story):
    return Flow(story).analyse()
