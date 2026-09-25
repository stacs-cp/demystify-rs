# Statistics for cheapest-move routes

For research on flow, insight and puzzle design, and proposed additional
measurements, see [Flow, insight and puzzle structure](FLOW-RESEARCH.md).

These measurements use the saved active DAG after MUS propagation reaches a
fixed point. Every outgoing move must use the current minimum discovered MUS
size. No more expensive moves, new solving states, or SAT searches are added.
All bounds are over complete routes ending in solved leaves.

```sh
# Single archive: JSON plus a readable report with witness routes.
python3 -B scripts/solve-corpus/analyse.py \
  --input path/to/puzzle.graph.json.zst --out puzzle.story.json.zst

# Whole corpus: update its SQLite index and write statistics/README.md.
python3 -B scripts/solve-corpus/analyse.py \
  --corpus etc/example-outputs/solve-graphs/all-games-2026-09-10

# Optional selection; omission includes all saved phases and instances.
python3 -B scripts/solve-corpus/analyse.py --corpus path/to/corpus \
  --phase baseline --ids gamut-plain-mus05-level-1

python3 -B -m unittest discover -s scripts/solve-corpus -p 'test_*.py' -v
```

Requires Python 3.9+ and the packages in `requirements.txt`.
`story.analyse(graph)` is the reusable in-memory API. The analysis never changes
graph archives. It rejects
unfinished checkpoints and reachable ambiguous/search-incomplete leaves rather
than treating their partial routes as complete. The batch driver also verifies
each archive against the generation database's recorded hash.

## Units and scope

One move is one MUS application, including its entire explicit deduction batch
and subsequent automatic closure. A proof deducing three cells counts once.
Constraint-free deductions already folded into a state are not separate moves.
The graph's assignment/elimination policy is retained in the result.

Multiple proofs with the same explicit deductions and successor are collapsed
for route calculations. Choice **width** is the number of distinct successor
states; the report also retains distinct-deduction, batch and explanation
widths. If two different explicit batches have the same successor, both remain
available to the release-depth calculation. `state_routes` counts unique state
sequences, independently of these explanation/batch alternatives. It is an
arbitrary-precision integer serialized as a decimal string, not a float.

Historical nodes and superseded edges are excluded. Every extremum is exact
for the saved graph and discovered evidence. The graph's randomized MUS search
does not certify global minimum explanations or exhaustive availability of all
possible explanations. Further searches can change these statistics. Source
pack tiers are not calibrated across different game encodings.

## Repeated hard moves and best/worst tie-breaking

For every threshold k from 1 to the largest active MUS size, `hard_moves`
records the minimum and maximum number of steps with MUS size at least k.
The minimum is unavoidable hard work across the permitted routes; the maximum
is the most a player can encounter by choosing differently among ties.
Their difference measures the effect of ordering at that threshold.

Each threshold also records the minimum and maximum longest consecutive run
of such moves. Total move count and peak MUS have their own bounds. Peak
invariance is measured rather than assumed. Thresholds can prefer different
routes: there is no invented single difficulty score or assertion that one
route achieves every minimum simultaneously. Every count, peak and streak
bound has its own full witness route.

Additive counts use dynamic programming over the DAG. Peak costs use the same
recurrence with max in place of addition. Maximum streaks use longest matching
suffixes; minimum streaks use bounded feasibility with monotone binary search.
No full-route enumeration is required.

## Choke points and narrow stretches

A choke point has width 1. Its MUS size says how hard the forced next move is.
`chokes` contains:

- Each choke state, its cost, separate widths, and whether that exact state lies
  on **every** complete route.
- A histogram of distinct active states by width and MUS size. These are state
  counts, not probabilities of a user visiting them.
- Min/max choke encounters per route at every MUS threshold, plus min/max
  hardest choke MUS (0 when a route has no choke point).
- Min/max encounters and longest consecutive runs at widths ≤1, ≤2 and ≤3.

Unavoidable **states** are found by exact prefix/suffix route counts. A puzzle
can force a choke somewhere on every route without any one choke state being
unavoidable: independent moves completed in different orders provide a simple
example. Encounter bounds capture this distinction.

## Release chains

Let c(s,L) be the smallest discovered proof size for literal L at state s,
using all saved evidence, including proofs larger than the current minimum.
An absent value means no discovered proof works there. It does not imply the
deduction is logically impossible.

For a deduction played at cost k, find the earliest state on that route where
c(s,L) ≤ k. If that is the root, its release depth is 1. Otherwise its depth is
one plus the depth of the move entering that first usable state. A batch's depth
is the maximum depth among its explicit deductions. A route's depth is the
largest batch depth encountered (0 for an already-solved root).

This follows chains across intervening independent moves. The implementation
updates per-literal depths whenever its best cost drops and carries unchanged
depths through waiting periods. At merged states, different histories can have
different depth vectors. Componentwise dominance discards only histories that
cannot improve the requested bound; incomparable vectors remain. Both the
minimum and maximum longest chain across complete routes are retained.

These are **path-specific last enabling events**. If A and B jointly make C
available, the event is whichever step completes those prerequisites on that
route. The result does not claim that this last step alone suffices, that every
chain link is unavoidable across all routes, or that it has recovered a full
AND/OR causal planning model. Alternate proofs are handled by the minimum over
all evidence, so merely finding a new same-size explanation does not create a
false release. The minimum route depth is the unavoidable depth under this
operational definition, not a certificate of individually necessary facts.

Reports distinguish cost drops from **queue promotions**: the latter occur
when a deduction becomes cheapest at the same cost it already had, because
easier choices were finished. They do not increase release depth. The driver
records min/max numbers of moves causing cost drops, cost drops that make a
deduction cheapest immediately, and queue promotions. One move can cause more
than one category for different deductions.

Each witness includes per-deduction enabling step, release depth and the number
of intervening moves it waited. `released_by_step: null` means available at the
root. JSON step indices are zero-based; Markdown step numbers are one-based.
Witnesses include proof IDs and original state/edge IDs for inspection in the
existing graph viewer.

Exact history-vector search can have exponentially many incomparable labels.
`--max-labels` defaults to 100,000 processed or simultaneously pending labels
per bound; 0 disables the limit. Reaching it marks the release bounds
`resource_limit` and the result `partial`; it never reports a truncated value
as an exact minimum/maximum. Other route/choke metrics remain exact. All 78
completed graphs in the first multi-game corpus fit within the default limit.

## Persistence and querying

`statistics/PHASE/ID.story.json.zst` is the machine-readable result and
`ID.story.md` contains tables and readable witness routes. The JSON records the
source graph hash, analyser source hashes, settings and computation time stamp.
`statistics/README.md` links the whole corpus. Re-running the command replaces
its generated statistics and is safe; run it again after graph refinement.

The SQLite database adds:

- `story_runs`: status, source/output hashes, report paths, route count and
  move/peak/chain bounds for each puzzle and phase.
- `story_thresholds`: hard-move count/gap, streak bounds, choke encounter bounds
  and count witness references for every difficulty threshold.
- `story_chokes`: costs, widths and unavoidable-state flags.
- `story_widths`: the width/cost histogram.
- `story_narrow`: encounter and streak bounds for widths ≤1, ≤2 and ≤3.

`current_story_statistics` joins source game/tier information and includes only
results whose source hash still matches the latest completed generation run.
It includes explicitly partial results; filter `status='complete'` when using
release-chain bounds. Checkpoints and ambiguous inputs are recorded as
`excluded`. Errors are recorded and cause a nonzero exit status. The analysis
tables do not change generation statuses or claim new puzzles were solved.

For example, to find cases where tie-breaking affects size-5-or-harder work:

```sql
SELECT s.game, s.puzzle_id, s.source_tier,
       t.hard_min, t.hard_max, t.hard_gap, s.report_path
FROM current_story_statistics s
JOIN story_thresholds t USING (puzzle_id, phase)
WHERE t.threshold = 5 AND t.hard_gap > 0
ORDER BY t.hard_gap DESC;
```

Tests cover known independent and linear cases, waiting across independent
moves, AND/OR enabling frontiers, duplicate proofs, batches, historical nodes,
incomplete input, a route count larger than 64 bits, and database invalidation.
An exhaustive small-graph oracle independently walks backwards to check
release depths and enumerates routes to verify all count and streak extrema.

## Payoff, pacing and metadata proxies

Statistics version 2 adds `flow`, computed by `flow.py`. Graph archives remain
version 1: none of the following requires regenerating or re-solving a puzzle.
The driver adds the new SQLite tables automatically and records the flow
analyser's source hash. JSON is compact for storage; Markdown remains the
human-readable report. Re-running replaces derived statistics, not graphs.

### Breakthroughs and endings

For every threshold k, hard means MUS size at least k and easy means size below
k. Each hard edge records newly easy unresolved deductions satisfying
`c(child,L) < k <= c(parent,L)`. Missing earlier evidence is treated as infinity
and counted separately as `previously_unexplained_literals`. This is evidence
availability, not a certificate that no earlier proof existed. Old easy work
remaining is a separate field; under the strict cheapest policy it must be zero
immediately after a hard move at that threshold.

The analyser gives min/max bounds over individual hard events for immediate
payoff and for the following cheap stretch, ending just before another hard
move or at completion. `between_hard_easy_moves` includes only stretches ending
at another hard move. No qualifying event is `not_applicable` with null bounds,
not a zero-length event. These event ranges are not bounds on each route's
average or worst payoff. Each bound has a full witness and a zero-based index of
the hard event within that witness.

`routine_tail_moves` and `routine_tail_gained_literals` measure work after the
last hard move. Routes with a hard move and routes without one are separate
partitions. The latter's suffix is the entire route. Bounds have full witnesses;
each partition also records its probability and conditional expected tail
under the policy below. When variable mapping is complete, following stretches
and tails also count newly assigned primary puzzle variables.

These are **temporal** stretches. They are not claimed to consist entirely of
descendants of the preceding hard move. Immediate cost-drop counts measure its
direct payoff; witness release annotations support inspection of later ancestry.
History-dependent aggregate cascade attribution remains future work.

Each edge separates explicit literals, automatic literals and their union.
Automatic progress includes structural closure and reveals. With complete
mapping, it also reports affected variables and newly assigned variables.
One variable is not assumed to equal one visible cell: encodings can have
multiple variables per cell or variables representing other puzzle objects.

### Local independence and reconvergence

A union diamond at u has successors a and b and a common immediate successor z
whose knowledge is exactly the union of a's and b's knowledge. Both two-step
orders must already be active cheapest routes. A stricter subset additionally
requires disjoint knowledge gains and the same cheapest cost at u, a and b.
Cost-changing diamonds are therefore visible without being classified as
unchanged-cost independence.

Local groups are connected components of the **non-commutation** relation
between choices, using that stricter subset. Choices in different groups
commute pairwise locally. This does not prove that their full future chains are
independent. A missing diamond also does not prove logical dependence: an
intermediate cheaper release can prevent an otherwise valid two-step ordering
under our policy. Reports include peak local-group bounds, multi-group
encounter bounds, and width-opening/closing counts over complete routes.

For every branching state, `first_common_state` is its nearest strict
postdominator: the first later state shared by all complete continuations. It
is null if none exists. Exact postdominators use topological bitsets. Per-state
diamond counts are complete; up to eight examples are saved, with an explicit
flag when more exist. Counts across all states are not visitation frequencies.

### Random ties and repetition

The named baseline selects a distinct cheapest successor uniformly, then a
distinct deduction batch reaching that successor uniformly. For profile
statistics it selects uniformly among the distinct family profiles of proofs
justifying that batch. Duplicate explanations do not multiply state choices or
identical profile options. This is an explicit modelling assumption, not an
estimate of observed player behaviour or a uniform distribution over complete
routes. Probabilities and expectations use floating-point arithmetic.

Forward probability propagation gives state visitation and expected work.
Backward recurrences give the probability of encountering at least one hard
move or hard choke. A probability-weighted tail recurrence conditions on the
presence of a hard move. Exact route extrema remain separately available.

A proof profile is the sorted multiset of its saved `family_of` constraint
names. These names are retained exactly, including instance numbers. A profile
is a **coarse constraint-family proxy**, not a recognized human technique or a
canonical structural proof. For each profile we compute expected uses, the
probability it occurs at least once, and whether it occurs on every possible
explanatory route. Summing encounter probabilities gives expected distinct
profiles; expected moves minus that sum gives expected repeated uses. This
avoids enumerating all histories. Alternative profiles for equivalent moves
are preserved. If any active proof lacks a family name, aggregate novelty
expectations are unavailable rather than treating unknown proofs as one family.

### What information is already saved?

| Measure | Existing information | Remaining limitation |
| --- | --- | --- |
| Payoff, tails, cheap intervals | Node evidence, known facts, edges and proof sizes | Relative to discovered MUSes; temporal stretches are not complete causal attribution |
| Local order flexibility and joins | Active DAG and known-fact sets | Global independence of multi-step blocks needs a stronger causal analysis |
| Easier work's visibility | Literal-to-variable maps and remaining deductions | Variable counts are not necessarily cell counts or perceptual salience |
| Repetition proxy | Constraint identities and `family_of` | Human strategy names and structural equivalence are not consistently annotated |
| Spatial display-target proxy | `constraint_metadata.targets` | Flat keys and mixed target kinds lack a common coordinate mapping; targets may omit parts of logical scope |
| Player-relative flow and enjoyment | No player observations in these graphs | Requires player experience, timings and subjective feedback |

`flow.metadata` records variable-mapping coverage. Incomplete mappings yield
null variable metrics instead of silent undercounts. Availability density is
reported separately for remaining target literals and unresolved mapped
variables. The literal denominator is `remaining`, not the number of all
candidate values in a player's interface.

Per-proof display targets are retained. Bounding boxes require every recorded
target to identify cells with integer coordinate pairs. The analyser does not
infer coordinates from flattened variable indices, mix non-cell targets into a
cell box, or call display-target geometry a complete proof footprint. Coverage
is recorded per proof, including when only some targets have coordinates.

Better spatial and strategy measures can be added as supplemental annotations
linked to the existing puzzle-variable and constraint IDs. Preserve coordinate
system/board-layer labels, stable rule kinds, and a versioned strategy
classifier. Frozen input and encoder sources in the corpus can help build these
mappings. New SAT searches are not inherently necessary for that enrichment.

### Additional query tables

- `story_flow`: metadata coverage, named random policy, expected work/repetition,
  and local-group bounds.
- `story_pacing`: per-threshold payoff, interval, tail and probability statistics.
- `story_availability`: per-state density and visitation probability.
- `story_independence`: per-state diamonds, local groups and first common state.
- `story_profiles`: family multisets, expected uses and encounter probabilities.

These tables cascade with replacement/exclusion of their parent `story_runs`
record. Join through `current_story_statistics` to exclude stale graph hashes;
old version-1 analyses have no rows in the new tables until reanalysed.

```sql
-- Compare payoff, endings and likely hard work at each puzzle's peak MUS.
SELECT s.game, s.puzzle_id, p.threshold,
       p.new_easy_min, p.new_easy_max, p.tail_min, p.tail_max,
       p.expected_hard, p.probability_hard_choke, s.report_path
FROM current_story_statistics s
JOIN story_pacing p USING (puzzle_id, phase)
WHERE p.threshold = s.peak_max
ORDER BY p.tail_max DESC;

-- States offering multiple locally independent groups, with visitation weight.
SELECT s.game, s.puzzle_id, i.state, i.local_groups, a.visit_probability
FROM current_story_statistics s
JOIN story_independence i USING (puzzle_id, phase)
JOIN story_availability a USING (puzzle_id, phase, state)
WHERE i.local_groups > 1
ORDER BY i.local_groups DESC;
```

Tests enumerate small DAG routes to check payoff, tails, cheap intervals,
conditional expectations, hard-choke probabilities, profile encounters and
postdominators. Additional cases cover duplicate proof/batch choices, automatic
closure, absent evidence, incomplete metadata, cost-changing commutation and
SQLite replacement/staleness.
