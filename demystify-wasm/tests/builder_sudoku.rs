//! WASM port of demystify-builder/tests/sudoku_4x4.rs.
//!
//! Builds a 4x4 sudoku via the wasm Builder bindings and checks the planner
//! deduces the unique reference solution.  Mirrors the Lua version.

use serde_wasm_bindgen::from_value;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

use demystify_wasm::{WasmBuilder, WasmPlanner, WasmPuzzle};

const N: i64 = 4;

const SOLUTION: [[i64; 4]; 4] = [[1, 2, 3, 4], [3, 4, 1, 2], [2, 1, 4, 3], [4, 3, 2, 1]];

const GIVENS: &[(i64, i64, i64)] = &[
    (1, 1, 1),
    (1, 4, 4),
    (2, 1, 3),
    (2, 4, 2),
    (3, 2, 1),
    (4, 3, 2),
];

const EXCLUSIONS: &[(i64, i64, i64)] = &[(1, 2, 1), (1, 2, 4), (4, 4, 3)];

fn cells_in_box(b: i64) -> Vec<(i64, i64)> {
    let br = ((b - 1) / 2) * 2 + 1;
    let bc = ((b - 1) % 2) * 2 + 1;
    let mut out = Vec::new();
    for dr in 0..2_i64 {
        for dc in 0..2_i64 {
            out.push((br + dr, bc + dc));
        }
    }
    out
}

fn dims_3d() -> JsValue {
    serde_wasm_bindgen::to_value(&vec![vec![1_i64, N], vec![1, N], vec![1, N]])
        .expect("encode 3-D dims")
}

fn dims_2d() -> JsValue {
    serde_wasm_bindgen::to_value(&vec![vec![1_i64, N], vec![1, N]]).expect("encode 2-D dims")
}

fn build_sudoku_4x4() -> WasmPuzzle {
    let b = WasmBuilder::new();
    b.kind("sudoku-4x4").expect("kind");

    let cell = b
        .var_bool_matrix("cell", dims_3d())
        .expect("declare cell matrix");

    // 1. Cell-exactly-one-value (unguarded).
    for r in 1..=N {
        for c in 1..=N {
            let lits: Vec<_> = (1..=N)
                .map(|v| cell.get(vec![r, c, v]).expect("cell index in range").pos())
                .collect();
            b.sum_eq_unguarded(lits, 1).expect("cell-exactly-one");
        }
    }

    // 2. Row uniqueness.
    let row_uniq = b
        .con_bool_matrix("row_uniq", dims_2d())
        .expect("row_uniq family");
    for r in 1..=N {
        for v in 1..=N {
            let lits: Vec<_> = (1..=N)
                .map(|c| cell.get(vec![r, c, v]).expect("cell").pos())
                .collect();
            let g = b
                .guard(
                    &row_uniq.get(vec![r, v]).expect("row_uniq atom"),
                    "row_uniq",
                    &format!("value {v} appears exactly once in row {r}"),
                )
                .expect("row_uniq guard");
            b.sum_eq(g, lits, 1).expect("row_uniq sum_eq");
        }
    }

    // 3. Column uniqueness.
    let col_uniq = b
        .con_bool_matrix("col_uniq", dims_2d())
        .expect("col_uniq family");
    for c in 1..=N {
        for v in 1..=N {
            let lits: Vec<_> = (1..=N)
                .map(|r| cell.get(vec![r, c, v]).expect("cell").pos())
                .collect();
            let g = b
                .guard(
                    &col_uniq.get(vec![c, v]).expect("col_uniq atom"),
                    "col_uniq",
                    &format!("value {v} appears exactly once in column {c}"),
                )
                .expect("col_uniq guard");
            b.sum_eq(g, lits, 1).expect("col_uniq sum_eq");
        }
    }

    // 4. Box uniqueness.
    let box_uniq = b
        .con_bool_matrix("box_uniq", dims_2d())
        .expect("box_uniq family");
    for bi in 1..=N {
        for v in 1..=N {
            let lits: Vec<_> = cells_in_box(bi)
                .into_iter()
                .map(|(r, c)| cell.get(vec![r, c, v]).expect("cell").pos())
                .collect();
            let g = b
                .guard(
                    &box_uniq.get(vec![bi, v]).expect("box_uniq atom"),
                    "box_uniq",
                    &format!("value {v} appears exactly once in box {bi}"),
                )
                .expect("box_uniq guard");
            b.sum_eq(g, lits, 1).expect("box_uniq sum_eq");
        }
    }

    // 5. Givens — single positive literal forced via sum_eq_unguarded.
    for &(r, c, v) in GIVENS {
        let lits = vec![cell.get(vec![r, c, v]).expect("given cell").pos()];
        b.sum_eq_unguarded(lits, 1).expect("given");
    }

    // 6. Exclusions — single negated literal via sum_eq_unguarded.
    for &(r, c, v) in EXCLUSIONS {
        let lits = vec![cell.get(vec![r, c, v]).expect("exclusion cell").neg()];
        b.sum_eq_unguarded(lits, 1).expect("exclusion");
    }

    b.build().expect("build")
}

#[wasm_bindgen_test]
fn builder_produces_three_con_families() {
    let puzzle = build_sudoku_4x4();
    assert_eq!(puzzle.kind().as_deref(), Some("sudoku-4x4"));

    // Three $#CON families (row_uniq / col_uniq / box_uniq).
    let cons: Vec<String> =
        from_value(puzzle.constraints().expect("constraints")).expect("decode constraints");
    assert_eq!(cons.len(), 3, "expected three families, got {cons:?}");

    // 3 families × N×N descriptions each.
    assert_eq!(puzzle.num_con_lits(), 3 * (N as usize) * (N as usize));
}

#[wasm_bindgen_test]
fn planner_deduces_full_solution() {
    let puzzle = build_sudoku_4x4();
    let planner = WasmPlanner::new(&puzzle).expect("planner");
    let _ = planner.quick_solve().expect("quick_solve");

    let known: Vec<String> = from_value(planner.known_literals().expect("known_literals"))
        .expect("decode known_literals");

    let raw = planner.current_state().expect("current_state");
    let state: serde_json::Value = from_value(raw.clone()).expect("decode current_state as Value");

    assert!(
        planner.is_solved(),
        "planner did not finish solving sudoku-4x4 \
         (known lits: {}, state: {state})",
        known.len()
    );

    let cell = state
        .get("cell")
        .unwrap_or_else(|| panic!("current_state has no 'cell' field; got {state}"));

    let mut grid = [[0_i64; 4]; 4];
    for (r_key, row) in cell.as_object().expect("cell is object") {
        let r: usize = r_key.parse::<usize>().expect("row key int") - 1;
        for (c_key, col) in row.as_object().expect("row is object") {
            let c: usize = c_key.parse::<usize>().expect("col key int") - 1;
            for (v_key, val) in col.as_object().expect("col is object") {
                if val.as_i64() == Some(1) {
                    let v: i64 = v_key.parse::<i64>().expect("val key int");
                    grid[r][c] = v;
                }
            }
        }
    }

    assert_eq!(
        grid, SOLUTION,
        "planner did not deduce the unique sudoku-4x4 solution"
    );
}
