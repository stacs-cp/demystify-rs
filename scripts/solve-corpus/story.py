#!/usr/bin/env python3
"""Exact route statistics on completed easiest-move DAGs; no SAT calls.

Release depth follows changes in the best *discovered* explanation for a
literal. A path-specific enabling event is not a counterfactual certificate
that a particular deduction is an individually necessary prerequisite.
"""

from dataclasses import dataclass
import collections
import flow

VERSION = 2


class InvalidGraph(ValueError):
    pass


class IncompleteGraph(ValueError):
    pass


class LabelLimit(RuntimeError):
    pass


@dataclass(frozen=True)
class Move:
    source: int
    edge: int
    target: int
    size: int
    deduced: tuple
    proofs: tuple

    @property
    def key(self):
        return (self.source, self.edge)


class Story:
    def __init__(self, graph):
        self.graph = graph
        if graph.get('format') != 'demystify-solve-graph' or graph.get('version') != 1:
            raise InvalidGraph('Unsupported graph format/version')
        if not graph.get('fixed_point'):
            raise IncompleteGraph('Checkpoint: finish proof propagation and exploration first')
        self.nodes = graph['nodes']
        self.root = graph['root']
        if not 0 <= self.root < len(self.nodes):
            raise InvalidGraph('Invalid root')
        self.sizes = [len(p['constraints']) for p in graph['proofs']]
        if any(size < 1 for size in self.sizes):
            raise InvalidGraph('Explicit moves must have positive MUS size')
        self.moves = {}
        self.best = {}
        self.by_key = {}
        queue = [self.root]
        seen = {self.root}
        for u in queue:
            node = self.nodes[u]
            if node['needs_expansion'] or node['checked_proofs'] != len(self.sizes):
                raise InvalidGraph('Fixed-point state has unfinished propagation/expansion')
            best = {}
            for proof, literals in node['evidence'].items():
                p = int(proof)
                if not 0 <= p < len(self.sizes):
                    raise InvalidGraph('Invalid proof index')
                for lit in literals:
                    if lit not in node['remaining'] or lit in node['known']:
                        raise InvalidGraph('Evidence contains a non-remaining deduction')
                    best[lit] = min(best.get(lit, self.sizes[p]), self.sizes[p])
            self.best[u] = best
            groups = collections.defaultdict(list)
            known = set(node['known'])
            for index, edge in enumerate(node['edges']):
                if not edge['active']:
                    continue
                v, p = edge['target'], edge['proof']
                if not 0 <= v < len(self.nodes) or not 0 <= p < len(self.sizes):
                    raise InvalidGraph('Invalid active edge index')
                if not known < set(self.nodes[v]['known']):
                    raise InvalidGraph('Edge must strictly increase knowledge')
                if not edge['deduced'] or not set(edge['deduced']) <= set(node['remaining']):
                    raise InvalidGraph('Invalid explicit deductions')
                if not set(edge['deduced']) <= set(node['evidence'].get(str(p), [])):
                    raise InvalidGraph('Edge deductions missing from proof evidence')
                if not best or self.sizes[p] != min(best.values()):
                    raise InvalidGraph('Active edge is not a cheapest move')
                groups[(v, tuple(edge['deduced']))].append((index, p))
                if v not in seen:
                    seen.add(v)
                    queue.append(v)
            moves = []
            for (v, deduced), explanations in groups.items():
                index, proof = explanations[0]
                move = Move(u, index, v, self.sizes[proof], deduced,
                            tuple(p for _, p in explanations))
                moves.append(move)
                self.by_key[move.key] = move
            self.moves[u] = sorted(moves, key=lambda m: m.edge)
            expected = {int(p) for p, ls in node['evidence'].items()
                        if ls and self.sizes[int(p)] == min(best.values())}
            actual = {p for move in moves for p in move.proofs}
            if expected != actual:
                raise InvalidGraph('Missing a cheapest explanation edge')
            if not moves and node['status'] != 'solved':
                raise IncompleteGraph(f"Reachable {node['status']} leaf at state {u}")
            if moves and node['status'] != 'open':
                raise InvalidGraph('Non-open state has outgoing moves')
            if not moves and (node['remaining'] or node['unresolved_candidates']):
                raise InvalidGraph('Solved leaf has unresolved deductions')
        self.order = sorted(seen, key=lambda u: (len(self.nodes[u]['known']), u))
        self.minimum = {u: min(self.best[u].values(), default=None) for u in self.order}
        self.width = {u: len({m.target for m in self.moves[u]}) for u in self.order}
        self.depth = {self.root: 0}
        self.prefix = {u: 0 for u in self.order}
        self.prefix[self.root] = 1
        self.suffix = {}
        for u in self.order:
            for v in {m.target for m in self.moves[u]}:
                self.prefix[v] += self.prefix[u]
                self.depth[v] = min(self.depth.get(v, self.depth[u] + 1), self.depth[u] + 1)
        for u in reversed(self.order):
            self.suffix[u] = (sum(self.suffix[v] for v in {m.target for m in self.moves[u]})
                              if self.moves[u] else 1)
        self.route_count = self.suffix[self.root]
        self.witnesses = {}
        self.route_ids = {}
        self.transitions = {}
        for u in self.order:
            for move in self.moves[u]:
                v = move.target
                released, promoted = [], []
                for lit, cost in sorted(self.best[v].items()):
                    old = self.best[u].get(lit)
                    if old is not None and cost > old:
                        raise InvalidGraph('Best proof cost increases on an active edge')
                    if old is None or cost < old:
                        released.append({'literal': lit, 'from_size': old, 'to_size': cost,
                                         'becomes_cheapest': cost == self.minimum[v]})
                    elif cost == self.minimum[v] and cost > self.minimum[u]:
                        promoted.append({'literal': lit, 'size': cost})
                self.transitions[move.key] = {'released': released, 'promoted': promoted}

    def intern(self, route):
        route = tuple(route)
        if route in self.route_ids:
            return self.route_ids[route]
        u = self.root
        steps = []
        for key in route:
            move = self.by_key[key]
            if move.source != u:
                raise InvalidGraph('Witness is not a continuous root route')
            steps.append({'state': u, 'edge': move.edge, 'target': move.target,
                          'mus_size': move.size, 'deduced': list(move.deduced),
                          'proof': self.nodes[u]['edges'][move.edge]['proof']})
            u = move.target
        if self.moves[u]:
            raise InvalidGraph('Witness does not reach a solved leaf')
        name = f'route-{len(self.witnesses)}'
        self.route_ids[route] = name
        self.witnesses[name] = {'steps': steps, 'terminal': u,
                                'difficulty_sequence': [s['mus_size'] for s in steps]}
        return name

    def extend(self, route, u):
        route = list(route)
        while self.moves[u]:
            move = self.moves[u][0]
            route.append(move.key)
            u = move.target
        return tuple(route)

    def extrema(self, weight, peak=False):
        """Min/max additive cost (or bottleneck cost), with full witness routes."""
        output = {}
        for largest, label in [(False, 'min'), (True, 'max')]:
            values, choices = {}, {}
            for u in reversed(self.order):
                candidates = []
                for move in self.moves[u]:
                    cost = weight(move)
                    value = max(cost, values[move.target]) if peak else cost + values[move.target]
                    candidates.append((value, move))
                if not candidates:
                    values[u] = 0
                    continue
                selected = (max if largest else min)(candidates, key=lambda x: x[0])
                values[u], choices[u] = selected
            route, u = [], self.root
            while u in choices:
                move = choices[u]
                route.append(move.key)
                u = move.target
            output[label] = values[self.root]
            output[label + '_route'] = self.intern(route)
        output['gap'] = output['max'] - output['min']
        return output

    def streak(self, predicate):
        """Extrema of the longest consecutive run of matching moves on a route."""
        run = {self.root: 0}
        paths = {self.root: ()}
        maximum, maximum_route = 0, self.extend((), self.root)
        for u in self.order:
            for move in self.moves[u]:
                value = run[u] + 1 if predicate(move) else 0
                route = paths[u] + (move.key,)
                if value > maximum:
                    maximum, maximum_route = value, self.extend(route, move.target)
                if move.target not in run or value > run[move.target]:
                    run[move.target], paths[move.target] = value, route

        def bounded(bound):
            # Among prefixes already obeying the bound, a smaller current run
            # dominates a larger one for every possible continuation.
            runs, prefixes = {self.root: 0}, {self.root: ()}
            for u in self.order:
                if u not in runs:
                    continue
                if not self.moves[u]:
                    return prefixes[u]
                for move in self.moves[u]:
                    value = runs[u] + 1 if predicate(move) else 0
                    if value <= bound and (move.target not in runs or value < runs[move.target]):
                        runs[move.target] = value
                        prefixes[move.target] = prefixes[u] + (move.key,)
            return None

        low, high = 0, maximum
        while low < high:
            middle = (low + high) // 2
            if bounded(middle) is None:
                low = middle + 1
            else:
                high = middle
        return {'min': low, 'max': maximum, 'gap': maximum - low,
                'min_route': self.intern(bounded(low)),
                'max_route': self.intern(maximum_route)}

    def chain(self, max_labels):
        """Exact extrema via componentwise dominance of release-depth labels.

        A label stores one depth per currently explained literal, plus the
        largest depth encountered. Earlier independent moves can be interleaved
        without breaking a dependency. A scalar per graph node is insufficient.
        """
        literals = {u: sorted(self.best[u]) for u in self.order}
        indices = {u: {lit: i for i, lit in enumerate(literals[u])} for u in self.order}
        transfers = {}
        for u in self.order:
            for move in self.moves[u]:
                changed = {r['literal'] for r in self.transitions[move.key]['released']}
                transfers[move.key] = (
                    [indices[u][lit] for lit in move.deduced],
                    [None if lit in changed else indices[u][lit] for lit in literals[move.target]],
                )
        result = {}
        for largest, name in [(False, 'min'), (True, 'max')]:
            labels = {self.root: {(0,) + (1,) * len(literals[self.root]): ()}}
            pending = 1
            best_final, best_route = None, None
            visited = 0
            for u in self.order:
                frontier = labels.pop(u, {})
                pending -= len(frontier)
                visited += len(frontier)
                if max_labels and visited > max_labels:
                    raise LabelLimit(f'Release-chain {name} exceeded {max_labels} processed labels')
                for vector, route in frontier.items():
                    if not self.moves[u]:
                        value = vector[0]
                        if best_final is None or (value > best_final if largest else value < best_final):
                            best_final, best_route = value, route
                        continue
                    for move in self.moves[u]:
                        inputs, outputs = transfers[move.key]
                        depth = max(vector[i + 1] for i in inputs)
                        candidate = (max(vector[0], depth),) + tuple(
                            depth + 1 if i is None else vector[i + 1] for i in outputs
                        )
                        bucket = labels.setdefault(move.target, {})
                        # Every continuation is monotone in all coordinates.
                        # Preserve incomparable histories, including AND batches.
                        def dominates(a, b):
                            return (all(x >= y for x, y in zip(a, b)) if largest
                                    else all(x <= y for x, y in zip(a, b)))
                        if any(dominates(existing, candidate) for existing in bucket):
                            continue
                        for existing in [e for e in bucket if dominates(candidate, e)]:
                            del bucket[existing]
                            pending -= 1
                        bucket[candidate] = route + (move.key,)
                        pending += 1
                        if max_labels and pending > max_labels:
                            raise LabelLimit(f'Release-chain {name} exceeded {max_labels} pending labels')
            result[name] = best_final
            result[name + '_route'] = self.intern(best_route)
            result[name + '_labels'] = visited
        result['gap'] = result['max'] - result['min']
        result['status'] = 'complete'
        return result

    def annotate_route(self, witness):
        """Reconstruct exact enabling events for every explicit deduction."""
        ranks = {lit: (1, None) for lit in self.best[self.root]}
        annotated = []
        for index, step in enumerate(witness['steps']):
            move = self.by_key[(step['state'], step['edge'])]
            causes = [{'literal': lit, 'depth': ranks[lit][0],
                       'released_by_step': ranks[lit][1],
                       'wait_moves': index - (ranks[lit][1] + 1 if ranks[lit][1] is not None else 0)}
                      for lit in move.deduced]
            depth = max(c['depth'] for c in causes)
            parents = sorted({c['released_by_step'] for c in causes
                              if c['released_by_step'] is not None})
            transition = self.transitions[move.key]
            annotated.append({'depth': depth, 'parents': parents, 'deductions': causes,
                              **transition})
            changed = {r['literal'] for r in transition['released']}
            ranks = {lit: ((depth + 1, index) if lit in changed else ranks[lit])
                     for lit in self.best[move.target]}
        longest = []
        if annotated:
            at = max(range(len(annotated)), key=lambda i: annotated[i]['depth'])
            while at is not None:
                longest.append(at)
                parents = annotated[at]['parents']
                at = max(parents, key=lambda i: annotated[i]['depth']) if parents else None
            longest.reverse()
        witness['release'] = {'longest_chain_steps': longest, 'depth': len(longest),
                              'events': annotated}

    def analyse(self, max_labels=100000):
        peak = self.extrema(lambda m: m.size, peak=True)
        thresholds = []
        for k in range(1, peak['max'] + 1):
            thresholds.append({'size_at_least': k,
                               **self.extrema(lambda m: int(m.size >= k)),
                               'longest_streak': self.streak(lambda m: m.size >= k)})
        choke_states, histogram = [], collections.Counter()
        for u in self.order:
            if not self.moves[u]:
                continue
            width = self.width[u]
            size = self.minimum[u]
            unavoidable = self.prefix[u] * self.suffix[u] == self.route_count
            histogram[(width, size)] += 1
            if width == 1:
                choke_states.append({
                    'state': u, 'mus_size': size, 'unavoidable_state': unavoidable,
                    'deduction_width': len({lit for m in self.moves[u] for lit in m.deduced}),
                    'batch_width': len(self.moves[u]),
                    'explanation_width': sum(len(m.proofs) for m in self.moves[u]),
                    'minimum_depth': self.depth[u],
                })
        narrow = []
        for width in [1, 2, 3]:
            predicate = lambda m: self.width[m.source] <= width
            narrow.append({'width_at_most': width,
                           'encounters': self.extrema(lambda m: int(predicate(m))),
                           'longest_streak': self.streak(predicate)})
        hard_chokes = [
            {'size_at_least': k,
             **self.extrema(lambda m: int(self.width[m.source] == 1 and m.size >= k))}
            for k in range(1, peak['max'] + 1)
        ]
        try:
            chains = self.chain(max_labels)
        except LabelLimit as error:
            chains = {'status': 'resource_limit', 'reason': str(error)}
        release = {
            'chains': chains,
            'cost_drop_moves': self.extrema(lambda m: int(bool(self.transitions[m.key]['released']))),
            'cheapest_release_moves': self.extrema(lambda m: int(any(
                r['becomes_cheapest'] for r in self.transitions[m.key]['released']))),
            'queue_promotion_moves': self.extrema(lambda m: int(bool(self.transitions[m.key]['promoted']))),
        }
        result = {
            'format': 'demystify-story-statistics', 'version': VERSION,
            'status': 'complete' if chains['status'] == 'complete' else 'partial',
            'policy': 'active easiest moves; equivalent explanations collapsed',
            'solver_policy': self.graph.get('solver_config', {}),
            'settings': {'max_release_labels_per_bound': max_labels},
            'evidence_scope': 'Discovered MUS fixed point; not certified global minima or causal prerequisites',
            'active_states': len(self.order), 'retained_states': len(self.nodes),
            'state_routes': str(self.route_count),
            'moves': self.extrema(lambda m: 1), 'peak_mus': peak,
            'hard_moves': thresholds,
            'chokes': {'states': choke_states, 'by_threshold': hard_chokes,
                       'peak_mus': self.extrema(lambda m: m.size if self.width[m.source] == 1 else 0,
                                                peak=True),
                       'width_size_histogram': [
                           {'width': w, 'mus_size': s, 'states': n}
                           for (w, s), n in sorted(histogram.items())],
                       'narrow_routes': narrow},
            'release': release, 'witnesses': self.witnesses,
        }
        result['flow'] = flow.analyse(self)
        for witness in self.witnesses.values():
            self.annotate_route(witness)
        if chains['status'] == 'complete':
            for which in ['min', 'max']:
                actual = self.witnesses[chains[which + '_route']]['release']['depth']
                if actual != chains[which]:
                    raise InvalidGraph('Release-chain witness disagrees with extremum')
        return result


def analyse(graph, max_labels=100000):
    return Story(graph).analyse(max_labels=max_labels)
