//! WASM port of the minesweeper-via-REVEAL test.  3x3 minesweeper,
//! `grid` source / `facts` reveal target, neighbour-count constraint
//! gated by `sumcheck ∧ facts` via `andAtom`.  `facts` is unconstrained
//! in CNF — the planner only learns it via the reveal cascade after
//! `grid[r, c]` is deduced.

use std::collections::{BTreeMap, BTreeSet};

use serde_wasm_bindgen::{from_value, to_value};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

use demystify_wasm::{WasmBuilder, WasmPlanner, WasmPuzzle};

const N: i64 = 3;

const CLUES: &[(i64, i64, i64)] = &[(1, 1, 0), (1, 2, 0), (2, 1, 0), (2, 2, 1)];
const EXPECTED: &[((i64, i64), i64)] = &[
    ((1, 3), 0),
    ((2, 3), 0),
    ((3, 1), 0),
    ((3, 2), 0),
    ((3, 3), 1),
];

fn dims_2d() -> JsValue {
    to_value(&vec![vec![1_i64, N], vec![1, N]]).expect("encode dims_2d")
}

fn dims_facts() -> JsValue {
    to_value(&vec![vec![1_i64, N], vec![1, N], vec![0, 1]]).expect("encode dims_facts")
}

fn build_minesweeper() -> WasmPuzzle {
    let b = WasmBuilder::new();
    b.kind("minesweeper-reveal-wasm").expect("kind");

    let grid = b
        .var_bool_matrix("grid", dims_2d())
        .expect("grid var matrix");
    b.show("grid", "main").expect("show grid main");

    let facts = b
        .reveal_bool_matrix("facts", dims_facts())
        .expect("facts reveal matrix");
    b.reveal("grid", "facts").expect("wire reveal");

    let sumcheck = b
        .con_bool_matrix("sumcheck", dims_2d())
        .expect("sumcheck $#CON");

    // Pin revealed cells to "not a mine" — these deductions trigger the
    // reveal cascade.
    for &(r, c, _) in CLUES {
        let signed = vec![grid.get(vec![r, c]).expect("grid clue cell").neg()];
        b.sum_eq_unguarded(signed, 1).expect("pin clue safe");
    }

    // Per-clue neighbour-count constraint: sumcheck is the $#CON,
    // facts is the reveal gate added via `gated_by` so the constraint
    // only fires once the cell has been deduced safe.
    for &(r, c, n_mines) in CLUES {
        let mut neighbours = Vec::new();
        for dr in -1..=1_i64 {
            for dc in -1..=1_i64 {
                if dr == 0 && dc == 0 {
                    continue;
                }
                let nr = r + dr;
                let nc = c + dc;
                if (1..=N).contains(&nr) && (1..=N).contains(&nc) {
                    neighbours.push(grid.get(vec![nr, nc]).expect("neighbour").pos());
                }
            }
        }
        let g = b
            .guard(
                &sumcheck.get(vec![r, c]).expect("sumcheck atom"),
                "sumcheck",
                &format!("exactly {n_mines} mines around ({r}, {c}) given safe"),
            )
            .expect("guard")
            .gated_by(&facts.get(vec![r, c, 0]).expect("facts atom"));
        b.sum_eq(g, neighbours, n_mines).expect("sumcheck sum_eq");
    }

    b.build().expect("build minesweeper")
}

#[wasm_bindgen_test]
fn set_reveal_rejects_unknown_source() {
    let b = WasmBuilder::new();
    b.reveal_bool_matrix("facts", dims_facts())
        .expect("declare facts");
    let err = b
        .reveal("nope", "facts")
        .expect_err("unknown source should error");
    let msg = format!("{err:?}");
    assert!(msg.contains("nope"), "error should name source: {msg}");
}

#[wasm_bindgen_test]
fn set_reveal_rejects_unknown_target() {
    let b = WasmBuilder::new();
    b.var_bool_matrix("grid", dims_2d()).expect("declare grid");
    let err = b
        .reveal("grid", "facts")
        .expect_err("unknown target should error");
    let msg = format!("{err:?}");
    assert!(msg.contains("facts"), "error should name target: {msg}");
}

#[wasm_bindgen_test]
fn difficulties_nonempty_on_minesweeper() {
    // Reproduces a bug reported by an embedder: on minesweeper-shaped
    // puzzles, WasmPlanner.difficulties() returns an empty object even
    // though bestStep / quickSolve find provable lits with size-1 MUSes.
    // difficulties() should report one entry per provable lit with the
    // smallest MUS size — at least equal in cardinality to the
    // bestStep / quickSolve output.
    let puzzle = build_minesweeper();
    let planner = WasmPlanner::new(&puzzle, wasm_bindgen::JsValue::NULL).expect("planner");

    let provable_count: usize = {
        let val = planner
            .provable_literals()
            .expect("provable_literals before difficulties");
        let v: Vec<String> = from_value(val).expect("decode provable_literals");
        v.len()
    };
    assert!(
        provable_count > 0,
        "minesweeper should have provable lits before any deduction"
    );

    let diffs: std::collections::BTreeMap<String, usize> =
        from_value(planner.difficulties().expect("difficulties")).expect("decode difficulties");
    assert!(
        !diffs.is_empty(),
        "difficulties() must not be empty when there are provable lits \
         (provable_count={provable_count})"
    );
    assert!(
        diffs.values().all(|&n| n >= 1),
        "every difficulty entry is a MUS size and so >= 1; got {diffs:?}"
    );
}

/// Builds a reveal-shaped minesweeper of arbitrary size with separate
/// lists of *pinned-safe* cells (CNF unit clauses, the player's starting
/// reveals) and *clued* cells (sumcheck constraints gated behind
/// `facts[r,c,0]`, only fired once the cell is deduced safe and the
/// reveal cascade activates).  The two lists need not coincide — a
/// player can know a cell is safe without yet knowing its clue number,
/// and a cell can have a clue we want active without being pre-revealed.
fn build_reveal_minesweeper(
    h: i64,
    w: i64,
    pinned_safe: &[(i64, i64)],
    clues: &[(i64, i64, i64)],
) -> WasmPuzzle {
    let dims2 = to_value(&vec![vec![1_i64, h], vec![1, w]]).expect("encode dims2");
    let dims_facts =
        to_value(&vec![vec![1_i64, h], vec![1, w], vec![0, 1]]).expect("encode dims_facts");

    let b = WasmBuilder::new();
    b.kind("reveal-minesweeper-fixture").expect("kind");

    let grid = b
        .var_bool_matrix("grid", dims2.clone())
        .expect("grid var matrix");
    b.show("grid", "main").expect("show grid main");

    let facts = b
        .reveal_bool_matrix("facts", dims_facts)
        .expect("facts reveal matrix");
    b.reveal("grid", "facts").expect("wire reveal");

    let sumcheck = b
        .con_bool_matrix("sumcheck", dims2)
        .expect("sumcheck $#CON");

    for &(r, c) in pinned_safe {
        let signed = vec![grid.get(vec![r, c]).expect("pinned cell").neg()];
        b.sum_eq_unguarded(signed, 1).expect("pin safe");
    }

    for &(r, c, n_mines) in clues {
        let mut neighbours = Vec::new();
        for dr in -1..=1_i64 {
            for dc in -1..=1_i64 {
                if dr == 0 && dc == 0 {
                    continue;
                }
                let nr = r + dr;
                let nc = c + dc;
                if (1..=h).contains(&nr) && (1..=w).contains(&nc) {
                    neighbours.push(grid.get(vec![nr, nc]).expect("neighbour").pos());
                }
            }
        }
        let g = b
            .guard(
                &sumcheck.get(vec![r, c]).expect("sumcheck atom"),
                "sumcheck",
                &format!("exactly {n_mines} mines around ({r}, {c}) given safe"),
            )
            .expect("guard")
            .gated_by(&facts.get(vec![r, c, 0]).expect("facts atom"));
        b.sum_eq(g, neighbours, n_mines).expect("sumcheck sum_eq");
    }

    b.build().expect("build reveal minesweeper")
}

/// True if `strs` contains a puzlit referring to cell `(r, c)` — matches
/// the `format_puzlit` shape for indexed vars, which renders Vec<i64>
/// indices via `Debug` as `[1, 2]` (comma-space).
fn mentions_cell<S: AsRef<str>>(strs: &[S], r: i64, c: i64) -> bool {
    let needle = format!("[{r}, {c}]");
    strs.iter().any(|s| s.as_ref().contains(&needle))
}

/// All cells in `[1, h] x [1, w]` mentioned by any puzlit string in the
/// known-literal list or difficulty-map keys.
fn visible_cells(planner: &WasmPlanner, h: i64, w: i64) -> BTreeSet<(i64, i64)> {
    let known: Vec<String> =
        from_value(planner.known_literals().expect("known")).expect("decode known");
    let diffs: BTreeMap<String, usize> =
        from_value(planner.difficulties().expect("diffs")).expect("decode diffs");
    let mut all_strs: Vec<String> = known;
    all_strs.extend(diffs.keys().cloned());
    let mut cells = BTreeSet::new();
    for r in 1..=h {
        for c in 1..=w {
            if mentions_cell(&all_strs, r, c) {
                cells.insert((r, c));
            }
        }
    }
    cells
}

#[wasm_bindgen_test]
fn difficulties_grow_progressively_minesweeper() {
    // 1x4 chain minesweeper. One starting reveal at (1,1), with clues
    // at (1,1)=0, (1,2)=0, (1,3)=1.  Only (1,1) is pinned safe via a
    // CNF unit clause; the others' clues are gated behind `facts`, so
    // they cannot fire until a deduction + reveal cascade activates
    // them.  The chain has to walk left-to-right one cell at a time:
    //
    //   init  →  (1,1) trivial-deduced + cascade activates clue (1,1)
    //            ⇒ (1,2) now provable, (1,3)/(1,4) still undecidable
    //   step1 →  (1,2) deduced + cascade activates clue (1,2)
    //            ⇒ (1,3) now provable
    //   step2 →  (1,3) deduced + cascade activates clue (1,3)
    //            ⇒ (1,4) now provable (forced mine by (1,3)=1)
    //   step3 →  (1,4) deduced
    //
    // The set of cells visible in `known_literals() ∪ difficulties()`
    // must therefore strictly grow by exactly one cell at each step.
    let puzzle = build_reveal_minesweeper(1, 4, &[(1, 1)], &[(1, 1, 0), (1, 2, 0), (1, 3, 1)]);
    let planner = WasmPlanner::new(&puzzle, wasm_bindgen::JsValue::NULL).expect("planner");

    let mut visible_at: Vec<BTreeSet<(i64, i64)>> = vec![visible_cells(&planner, 1, 4)];

    let expected_initial: BTreeSet<(i64, i64)> = [(1, 1), (1, 2)].into_iter().collect();
    assert_eq!(
        visible_at[0], expected_initial,
        "initial visible cells (after trivial-deduce + 1 cascade): \
         only the pinned cell and the next chain-link should be reachable"
    );

    let mut step_count = 0;
    loop {
        let step = planner.best_step().expect("bestStep");
        if step.is_null() {
            break;
        }
        step_count += 1;
        assert!(step_count < 10, "1x4 chain should solve in a few steps");
        visible_at.push(visible_cells(&planner, 1, 4));
    }

    // Monotonic: nothing visible at step i may disappear at step i+1.
    // (A cell can only leave `difficulties()` by being deduced, in which
    // case it enters `known_literals()` — `visible_cells` covers both.)
    for i in 1..visible_at.len() {
        assert!(
            visible_at[i].is_superset(&visible_at[i - 1]),
            "step {i}: visible cells must not regress: {:?} → {:?}",
            visible_at[i - 1],
            visible_at[i],
        );
    }

    // The chain has to progress strictly somewhere — otherwise the
    // reveal cascade isn't doing anything.  We saw {(1,1), (1,2)}
    // initially, and we expect to reach {1..4} via at least two
    // distinct intermediate sizes.  (The very last step typically
    // doesn't grow visible_cells: the deduced cell was already in
    // `difficulties()` before this step, so it just moves from diffs
    // to known and the union stays the same size.)
    let distinct: BTreeSet<&BTreeSet<(i64, i64)>> = visible_at.iter().collect();
    assert!(
        distinct.len() >= 3,
        "chain didn't progress through enough intermediate states \
         (saw {} distinct snapshots): {visible_at:?}",
        distinct.len()
    );

    assert!(planner.is_solved(), "1x4 chain should solve fully");
    let all_cells: BTreeSet<(i64, i64)> = [(1, 1), (1, 2), (1, 3), (1, 4)].into_iter().collect();
    assert_eq!(visible_at.last().unwrap(), &all_cells);
}

#[wasm_bindgen_test]
fn difficulties_omit_undecidable_cell_minesweeper() {
    // 1x3 grid with a single clue at (1,1) saying "1 mine adjacent".
    // The only neighbour is (1,2), so grid[1,2] is forced to be a
    // mine.  grid[1,3] is referenced by *no* constraint, so it's
    // genuinely undecidable: the SAT problem has models with it true
    // and models with it false.  difficulties() — which is supposed
    // to be current-state-aware — must never report grid[1,3].
    //
    // Note: (1,1) is trivially-deduced at planner construction (its
    // pin is a CNF unit clause), so by the time we read the first
    // `difficulties()`, (1,1) is already in `known_literals()` and
    // the reveal cascade has activated the (1,1) sumcheck — making
    // (1,2)=mine immediately provable, not (1,1).
    let puzzle = build_reveal_minesweeper(1, 3, &[(1, 1)], &[(1, 1, 1)]);
    let planner = WasmPlanner::new(&puzzle, wasm_bindgen::JsValue::NULL).expect("planner");

    let initial_known: Vec<String> =
        from_value(planner.known_literals().expect("initial known")).expect("decode initial known");
    let initial_diffs: BTreeMap<String, usize> =
        from_value(planner.difficulties().expect("initial diffs")).expect("decode initial diffs");
    let initial_keys: Vec<String> = initial_diffs.keys().cloned().collect();

    assert!(
        mentions_cell(&initial_known, 1, 1),
        "(1,1) is trivially-deduced at planner construction: {initial_known:?}"
    );
    assert!(
        !mentions_cell(&initial_keys, 1, 1),
        "(1,1) is already known, so it must not appear in difficulties(): {initial_keys:?}"
    );
    assert!(
        mentions_cell(&initial_keys, 1, 2),
        "(1,2) must be provable from the now-active (1,1) sumcheck: {initial_keys:?}"
    );
    assert!(
        !mentions_cell(&initial_keys, 1, 3),
        "(1,3) is undecidable, must never appear in difficulties(): {initial_keys:?}"
    );

    let mut step_count = 0;
    while !planner.best_step().expect("bestStep").is_null() {
        step_count += 1;
        assert!(step_count < 10, "1x3 minesweeper should converge quickly");
    }

    assert!(
        !planner.is_solved(),
        "grid[1,3] is undecidable, so is_solved() must be false"
    );

    let final_diffs: BTreeMap<String, usize> =
        from_value(planner.difficulties().expect("final diffs")).expect("decode final diffs");
    let final_keys: Vec<String> = final_diffs.keys().cloned().collect();
    let known: Vec<String> =
        from_value(planner.known_literals().expect("known")).expect("decode known");

    assert!(
        !mentions_cell(&final_keys, 1, 3),
        "final difficulties() must omit the undecidable grid[1,3]: {final_keys:?}"
    );
    assert!(
        !mentions_cell(&known, 1, 3),
        "grid[1,3] must remain unknown after all steps: {known:?}"
    );
    assert!(
        mentions_cell(&known, 1, 1),
        "grid[1,1] should be deduced (safe via pin): {known:?}"
    );
    assert!(
        mentions_cell(&known, 1, 2),
        "grid[1,2] should be deduced (mine via the (1,1)=1 clue): {known:?}"
    );
}

#[wasm_bindgen_test]
fn planner_deduces_minesweeper_via_reveal() {
    let puzzle = build_minesweeper();
    let planner = WasmPlanner::new(&puzzle, wasm_bindgen::JsValue::NULL).expect("planner");
    let _ = planner.quick_solve().expect("quick_solve");
    assert!(
        planner.is_solved(),
        "minesweeper-via-REVEAL should solve fully"
    );

    let state: serde_json::Value =
        from_value(planner.current_state().expect("current_state")).expect("decode state");
    let grid = state
        .get("grid")
        .unwrap_or_else(|| panic!("state missing 'grid' field: {state}"));

    for &((r, c), want) in EXPECTED {
        let got = grid
            .get(r.to_string())
            .and_then(|row| row.get(c.to_string()))
            .and_then(|v| v.as_i64())
            .unwrap_or_else(|| panic!("grid[{r},{c}] missing in state: {state}"));
        assert_eq!(got, want, "grid[{r},{c}]: expected {want}, got {got}");
    }
}
