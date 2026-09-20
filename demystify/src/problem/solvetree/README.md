# Persistent solve-graph research

`demystify-solvetree` explores easiest-move routes through small puzzles and
records when discovered explanations could have been used. It does **not**
compute a difficulty heatmap for every remaining square at every state.

## Build and save

From the workspace root:

```sh
cargo build --release -p demystify --bin demystify-solvetree

target/release/demystify-solvetree build \
  --model eprime/futoshiki.eprime \
  --param eprime/futoshiki/puzzle-futoshiki-com/4x4-easy-01.param \
  --only-assign --repeats 5 --out futoshiki.graph.json

target/release/demystify-solvetree build \
  --model eprime/ppminesweeper.eprime \
  --param demystify/tst/solvetree-4x5.param \
  --out minesweeper.graph.json
```

`--only-assign` explores assignments rather than individual candidate
eliminations. This is a different move policy and is saved in the archive.
Without it, the normal demystify deduction targets are used. Applying a proof
makes every remaining deduction that this proof is known to justify at that
state; different proofs with identical consequences retain separate edges to
the same state. Structural size-zero deductions, implied eliminations and
reveals are included in the state closure. Their extra facts are recorded on
edges as `implied`; structural facts already known at the start are in the root.

Tune discovery effort with `--repeats`, `--strategy quick|slice|cake|dynamic`,
`--conflict-limit` (0 means unlimited), and `--threads`. Propagation validity and
subset-reduction checks are always unlimited: a timeout must not be mistaken
for an explanation being unavailable.

The web **Solve Tree Explorer** also offers **Research (small puzzles)** mode,
with a search count and a **Save research graph** button. It starts at the
session's initial puzzle state, preserving pinned givens. Compact exploration
keeps the old merging behavior. Use the CLI for checkpointed overnight work.

## Overnight batches

```sh
target/release/demystify-solvetree build \
  --model eprime/futoshiki.eprime \
  --param eprime/futoshiki/puzzle-futoshiki-com/4x4-easy-*.param \
  --only-assign --repeats 5 --threads 4 --out-dir graphs
```

Each puzzle gets `<parameter-stem>.graph.json`. Duplicate output names are
rejected before starting. A failed puzzle does not stop subsequent puzzles;
`batch-summary.json` is updated after each one, summaries are printed as JSON
lines on stdout, and the process exits unsuccessfully if any puzzle failed.
Progress goes to stderr. Existing graph files are protected unless explicitly
replaced with `--overwrite`.

An initial checkpoint is saved after parsing and root normalization, and then
after units of work whenever `--checkpoint-seconds` has elapsed (default 30;
0 saves after every unit). A unit is a state search/expansion or a state's
propagation work, so expensive units can exceed that interval. Replacement is
atomic; interruption leaves the previous checkpoint intact. There is no node,
depth or wall-clock cutoff in this initial version.

## Resume or search harder

```sh
# Complete unfinished work, retaining search results and checked proof/state pairs.
target/release/demystify-solvetree resume --input futoshiki.graph.json

# Another discovery round, followed by propagation and exploration until stable.
target/release/demystify-solvetree resume --input futoshiki.graph.json \
  --repeats 20 --conflict-limit 0 --out futoshiki-refined.graph.json

# Another round using the previous search settings.
target/release/demystify-solvetree resume --input futoshiki.graph.json --search-more
```

Plain `resume` on a completed archive does no new searching. Changes to repeats,
strategy or conflict limit imply a new discovery round. Every requested round's
settings are retained. New rounds include historical states, since their
explanations can still improve the active graph.

`build --load-parsed puzzle.json` skips the external compiler. For generated
puzzles use `--pin-assignment assignment.json`; it accepts a bare assignment or
a mystify output containing `puzzle`. The resulting pinned facts are saved in
the root. Resume needs neither the original files nor Conjure.

### Fell and Mauve level JSON

The native builder example accepts shipped Fell/Mauve level files and exports
the parsed puzzle, using the games' cell domains and constraint grouping:

```sh
cargo build --release -p demystify-builder --example graph_corpus
target/release/examples/graph_corpus fell level.json fell.parsed.json
# Use `mauve` for Mauve level files (lightsThroughOpposite must be true).
target/release/demystify-solvetree build --load-parsed fell.parsed.json \
  --only-assign --repeats 5 --out fell.graph.json
```

The exporter checks that every value in the supplied reference solution is
uniquely implied before saving. Fell uses one height variable per tile, one
constraint per edge and one per label. Mauve uses one empty/red/blue variable
per cell, with separate sightline, coverage, clue-count and opposite-colour-ban
constraints. Walls and givens are structural. Source pack difficulty labels
are not measurements under this encoding; compare the resulting graphs.

## Study later, offline

```sh
target/release/demystify-solvetree inspect --input futoshiki.graph.json
target/release/demystify-solvetree render \
  --input futoshiki.graph.json --out futoshiki.html
```

Open the HTML in a browser. It is self-contained and makes no network requests.
Follow moves to see their difficulty sequence, select a proof and a deduction
to inspect all states where that explanation worked, and jump to the beginnings
of its availability on different branches. **Include superseded routes** exposes
the historical graph. The viewer can also open other saved JSON archives.

## Algorithm and interpretation

For release-chain depth, choke widths/difficulties, and best/worst hard-move
counts across cheapest-move ties, use the offline
[story statistics tool](../../../../scripts/solve-corpus/STORY.md). It reads
these archives without SAT calls and saves JSON, witness routes and corpus
database tables. It rejects unfinished graphs for route comparisons.

1. Search each new state for its smallest explanations using the configured
   randomized search budget. Keep all discovered witnesses, including ties.
2. Explore every distinct current minimum-size proof. Canonical sorted fact
   sets merge states reached in different orders; no lattice/size-one batching
   hides intermediate moves.
3. Sweep states from more knowledge to less, then from less to more. For each
   previously unchecked `(state, proof)` pair, test the discovered constraint set
   against each remaining deduction using **only that state's facts**. Retain
   successes and deterministically minimize their constraint subsets. Newly
   reduced proofs join the shared pool. Checking the shared pool also transfers
   explanations to sibling branches even if their common ancestor cannot use them.
4. Re-expand changed states. Equally easy proofs add alternatives; smaller proofs
   replace the active easiest choices. Explore new states and repeat until all
   retained states have been searched, all proof/state pairs checked, and all
   affected outgoing edges repaired.

The growing evidence is finite. Best known costs can only decrease, while
same-size discoveries can still add branches. An individual proof's availability
is monotone as knowledge is added; it does not establish the earliest possible
availability of *every* explanation for that deduction. The availability
frontier is branch-specific rather than one depth in the shared-state DAG.

`fixed_point: true` means closure of the accumulated evidence and exploration
policy. It is **not** exhaustive MUS enumeration or a proof of globally minimum
costs. Further random searches can discover another proof and restart the loop.
Leaf statuses distinguish `solved`, `underdetermined` (no further deductions but
unresolved candidates), and `search_incomplete` (provable deductions exist but
discovery found no explanation within its budget).

## Archive schema (version 1)

The top-level `format` is `demystify-solve-graph`. The JSON includes the producer
version, source label, complete parsed puzzle/CNF, solver target policy, search
settings, nodes, proofs and propagation progress. It uses signed DIMACS integers
for literals and constraints; the embedded puzzle maps them back to names.

- Node/proof ids are their indices in `nodes`/`proofs`, stable across saves and
  further discovery rounds. States are deduplicated by the full fact vector,
  not a potentially colliding hash.
- `nodes[i].evidence[proof_id]` lists deductions justified by that proof here.
  It includes larger valid explanations even after smaller ones are discovered.
  It never lists an already-known deduction, which would give trivial proofs.
- A proof stores its constraint set, first discovery state and optional source
  proof when obtained by reduction. It can be valid but no longer minimal at
  another state; its size remains the cost of *that explanation*.
- Edges store proof id, target state, explicit deductions, automatic extra facts
  and `active`. Old edges are retained. The current solve graph consists only of
  active edges reachable from `root`; an active edge at an unreachable historical
  state does not make that state part of the current graph.
- `searched_round`, `checked_proofs` and `needs_expansion` allow interrupted
  archives to resume. A checkpoint's edges can still need repair; use the
  completed fixed point for comparisons.

`SolveGraph::availability_frontier(proof, deduction)` supplies the starts on
current routes; `depths()` supplies shortest root distances (`None` for historical
states). These are evidence for future pinch-point and story metrics, not an
overall puzzle-quality score. Count distinct deductions or successor states
separately from the number of explanations when measuring choice width.
