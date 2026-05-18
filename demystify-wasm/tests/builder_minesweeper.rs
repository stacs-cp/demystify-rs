//! WASM port of the minesweeper-via-REVEAL test.  3x3 minesweeper,
//! `grid` source / `facts` reveal target, neighbour-count constraint
//! gated by `sumcheck ∧ facts` via `andAtom`.  `facts` is unconstrained
//! in CNF — the planner only learns it via the reveal cascade after
//! `grid[r, c]` is deduced.

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

    // Per-clue neighbour-count constraint, gated by sumcheck ∧ facts.
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
        let gate = b
            .and_atom(vec![
                sumcheck.get(vec![r, c]).expect("sumcheck atom").pos(),
                facts.get(vec![r, c, 0]).expect("facts atom").pos(),
            ])
            .expect("and_atom gate");
        let g = b
            .guard(
                &gate,
                "sumcheck",
                &format!("exactly {n_mines} mines around ({r}, {c}) given safe"),
            )
            .expect("guard");
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
fn planner_deduces_minesweeper_via_reveal() {
    let puzzle = build_minesweeper();
    let planner = WasmPlanner::new(&puzzle).expect("planner");
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
