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

## Migrating `mystify`

`mystify` currently has its own copy of the assignment-pinning logic. It
should call into demystify instead:

- **`mystify/src/corpus.rs`** — `apply_puzzle_clues` + `walk_assign` walk the
  saved `"puzzle"` JSON and call `solver.add_not_provable_known_lit` per leaf.
  Replace the `apply_puzzle_clues(&mut planner, puzzle_clues)?` call in
  `build_planner` with

  ```rust
  planner.solver().pin_assignment(puzzle_clues)?;
  ```

  and delete `apply_puzzle_clues` and `walk_assign`. (`puzzle_clues` is the
  `"puzzle"` sub-object — `saved.json["puzzle"]`; `pin_assignment` pins every
  variable it finds, which is correct because that object only ever contains
  the `puz_*` clue cells. If a future format puts non-clue keys there, slice
  first.) `mystify` keeps its own `PuzzleContext::from_json` / `make_planner`
  parsing path — only the *pinning* moves.

- **`mystify/src/puzzle.rs`** — `PuzzleDesign::apply_to_planner` /
  `apply_to_planner_impl` already do the minimal thing (iterate `(PuzVar, val)`
  pairs, call `add_not_provable_known_lit`) and don't go through JSON, so they
  need no change. `apply_to_planner_neighbourhood` (the `neighbourhood_`-prefixed
  variant) is generation-specific and stays as is. If you later want a single
  code path, `PuzzleDesign` could expose its assignment as a `PuzVar::to_json_map`
  and feed it to `pin_assignment`, but that is optional.

After migrating, `mystify` inherits the input validation for free, and the
demystify dependency must be at a revision that includes `pin_assignment`.

[`PuzVar::to_json_map`]: ../demystify/src/problem/mod.rs
[`PuzzleSolver::pin_assignment`]: ../demystify/src/problem/solver.rs
[`parse_essence_with_assignment`]: ../demystify/src/problem/parse.rs
[`mystify_puzzle_assignment`]: ../demystify/src/problem/parse.rs
