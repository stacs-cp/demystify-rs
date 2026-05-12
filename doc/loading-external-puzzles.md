# Loading externally-generated puzzles

A normal demystify puzzle is an Essence model plus a `.param` file: the
`given` parameters in the `.param` *are* the puzzle instance (the Sudoku
givens, the Killer cages, …).

Puzzle *generators* — `mystify`, in this workspace — work differently. To
search the space of puzzles, the generator declares the clue cells as `find`
**variables**, not `given` parameters, so that local search can mutate them.
A concrete instance produced by such a search is then an *assignment* to those
variables. An Essence `.param` file cannot assign `find` variables, so the
instance has to be supplied separately, as a JSON assignment object.

This page documents the demystify-side machinery for that, and how `mystify`
should use it.

## The assignment JSON

An assignment is a JSON object mapping each variable name to a nested
`index → … → integer` object — exactly the shape
[`PuzVar::to_json_map`] produces:

```json
{
  "puz_grid":   { "1": { "1": -1, "2": 5, "3": -1, "4": 4, "5": -1 }, "2": { … }, … },
  "puz_colour": { "1": { "1": 2,  "2": 0, … }, … }
}
```

meaning `puz_grid[1,1] = -1`, `puz_grid[1,2] = 5`, `puz_colour[1,1] = 2`, …
Every integer leaf becomes a known equality literal. In `mystify`'s
saved per-puzzle JSON this object lives under the top-level `"puzzle"` key
(alongside `"params"`, `"solution"`, `"difficulty"`, …).

## API

| What | Function |
|---|---|
| Pin an assignment onto an already-parsed solver | [`PuzzleSolver::pin_assignment`] (`problem::solver`) |
| Parse `.eprime` + `.param`, then pin an assignment | [`parse_essence_with_assignment`] (`problem::parse`) |
| Pull the assignment out of a `mystify` output JSON | [`mystify_puzzle_assignment`] (`problem::parse`) |
| Same, from the CLI | `demystify --pin-assignment <json>` |

`pin_assignment` validates the whole assignment *before* mutating anything:
it fails (leaving the solver untouched) if the value is not a JSON object,
has a non-integer index key, has a non-integer leaf, names a variable the
model does not declare, or gives a value outside that variable's domain.
`parse_essence_with_assignment` additionally fails if the pinned assignment
makes the model unsatisfiable.

`pin_assignment` pins via `add_not_provable_known_lit` — the assignment is
treated as an axiom the solver is *not* required to prove. Pin before
constructing the `PuzzlePlanner` so its initial trivial-deduction pass sees
the givens (pinning afterwards also works).

### CLI

```
demystify --model jigsawminesweeper.eprime \
          --param jigsawminesweeper.param \
          --pin-assignment seed-009.json
```

`--pin-assignment` accepts either a bare assignment object or a full
`mystify` output JSON (in which case the assignment is read from its
top-level `"puzzle"` key).

## What happens to the generation machinery

A generator model carries more than the clue cells: in `mystify`'s case a
local-search *neighbourhood* (`neighbourhood_*`, `distance_neighbourhood`),
a `demystify_tidiness_measure`, a `demystify_enforce_design` switch, etc.
These are all `$#AUX` variables. Once the `$#VAR puz_*` clue cells are
pinned, that machinery is inert — the `neighbourhood_*` variables are
otherwise unconstrained, `distance_neighbourhood` follows from them, and none
of it touches the user variable being deduced. So the same model file serves
both roles: generation (no pinning) and solving/explaining (pinned). There is
no need for a separate "solver-only" copy of the model — and keeping one
would mean two copies of the constraint semantics that must stay in lock-step.

## `mystify` side

`mystify/src/corpus.rs::build_planner` already pins via
`planner.solver().pin_assignment(saved.json["puzzle"])` (its old
`walk_assign`/`apply_puzzle_clues` copy is gone). `PuzzleDesign::apply_to_planner`
in `mystify/src/puzzle.rs` still iterates `(PuzVar, val)` pairs and calls
`add_not_provable_known_lit` directly — that's the same primitive without the
JSON detour, so it needs no change; the `neighbourhood_`-prefixed variant is
generation-specific and stays. `mystify` keeps its own `PuzzleContext::from_json`
/ `make_planner` parsing path — only the *pinning* came across.

### Colour-region tinting

The jigsaw-minesweeper models declare their colour partition with
`$#SHOW puz_colour cages`, which draws thick borders between adjacent cells of
different colour and treats colour `0` as a real cage. The `region_tint`
`$#SHOW` role (added for this — see `ShowRole` in `demystify/src/problem/parse.rs`)
is the right fit: it tints coloured cells, honours `0` as "uncoloured", and
draws no borders. The demystify-side test fixtures
(`demystify/tst/{pp,}jigsawminesweeper.eprime`) already use `region_tint`; the
`mystify`-side `examples/{pp,}jigsawminesweeper.eprime` should switch
`$#SHOW puz_colour cages` → `$#SHOW puz_colour region_tint` to match.

[`PuzVar::to_json_map`]: ../demystify/src/problem/mod.rs
[`PuzzleSolver::pin_assignment`]: ../demystify/src/problem/solver.rs
[`parse_essence_with_assignment`]: ../demystify/src/problem/parse.rs
[`mystify_puzzle_assignment`]: ../demystify/src/problem/parse.rs
